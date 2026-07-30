//! Parser — tokens to AST.
//!
//! Port of `AstBuilder.java` (1,587 LOC) and the document-assembly half of
//! `EquationParser.java` (3,042 LOC), against the grammar in `Frees.g4`.
//!
//! The module is split so the expression grammar and the statement/top-level
//! grammar can be worked on independently:
//!
//! * [`expr`] — `boolExpr` / `expr` / `addExpr` / `mulExpr` / `unaryExpr` /
//!   `powExpr` / `atom`, plus `argList`, `arrayIndexList`, matrix literals and
//!   unit annotations.
//! * [`toplevel`] — `program` / `topLevel` / `statement` and the block forms.

pub mod complex;
pub mod defs;
pub mod expand;
pub mod expr;
pub mod latex;
pub mod toplevel;

use std::collections::BTreeMap;

use crate::ast::Statement;
use crate::diag::{Diagnostic, FreesError, Result, Span};
use crate::token::{Token, TokenKind};

pub use expr::{parse_bool_expr, parse_expr};
pub use toplevel::parse_document;

/// An in-text `GUESS` directive: the initial guess and/or bounds that travel
/// with the document. Port of `ast/GuessDirective.java`.
#[derive(Debug, Clone, PartialEq)]
pub struct GuessDirective {
    /// Variable name, lowercased.
    pub name: String,
    pub guess: Option<f64>,
    pub lower: Option<f64>,
    pub upper: Option<f64>,
}

/// A parsed document.
///
/// Block constructs that the wasm port has not reached yet (`PARAMETRIC`,
/// `PLOT`, `STATE TABLE`, `DYNAMIC`, `COMPONENT`, `connect`, `LINEARIZE`) are
/// reported as an explicit unsupported-construct error rather than being
/// silently skipped — a wrong answer is worse than a refusal. `FUNCTION`,
/// `PROCEDURE`, `MODULE` and `TABLE` parse into [`Document::defs`] (Phase 4).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub statements: Vec<Statement>,
    pub guesses: Vec<GuessDirective>,
    pub diagnostics: Vec<Diagnostic>,
    /// `FUNCTION` / `PROCEDURE` / `MODULE` / `TABLE` definitions, in
    /// declaration order.
    pub defs: defs::Definitions,
    /// Lowercase canonical name → the spelling of its **first appearance as a
    /// variable**, exactly as the Java `AstBuilder` accumulates
    /// `ParseResult.displayNames`.
    ///
    /// Registered at two sites only, mirroring `AstBuilder.visitVarAtom` and
    /// `AstBuilder.visitArrayAtom`: a bare identifier atom that is not a
    /// built-in `#` constant, and the base name of an array access. Identifiers
    /// the Java visitor never turns into an `Expr.Var` — unit spellings inside
    /// `[…]`, called function names, `FUNCTION`/`TABLE` headers and their
    /// formal parameters, dotted member paths (`visitMemberAtom` does *not*
    /// register) — are deliberately absent, which is why a `[J/kg-K]` cannot
    /// steal the display name of a later `k`.
    pub display_names: BTreeMap<String, String>,
}

impl Document {
    /// Every equation in the document, flattened out of `FOR` blocks.
    pub fn equations(&self) -> Vec<&crate::ast::Equation> {
        fn walk<'a>(stmts: &'a [Statement], out: &mut Vec<&'a crate::ast::Equation>) {
            for s in stmts {
                match s {
                    Statement::Eq(eq) => out.push(eq),
                    Statement::For { body, .. } => walk(body, out),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.statements, &mut out);
        out
    }
}

/// A cursor over the token stream with the lookahead the grammar needs.
///
/// The frees grammar needs more than one token of lookahead in a few places —
/// `componentInst` (`IDENT IDENT (`) versus an equation starting with an
/// identifier, and `multiAssign` (`[ … ] =`) versus a matrix-literal equation.
/// [`Cursor::peek_at`] exists for exactly those decisions.
pub struct Cursor<'a> {
    tokens: &'a [Token],
    pos: usize,
    source: &'a str,
    /// Accumulates [`Document::display_names`] as the atoms are parsed — the
    /// port of the `AstBuilder`'s own `displayNames` field. First spelling
    /// wins (`putIfAbsent`).
    display_names: BTreeMap<String, String>,
    /// The lowercase key of every display name a registration actually
    /// *inserted*, in order. Lets a caller ask "did the region I just parsed
    /// introduce this spelling?" without cloning the map — see
    /// [`Cursor::display_name_mark`].
    inserted_order: Vec<String>,
}

impl<'a> Cursor<'a> {
    pub fn new(tokens: &'a [Token], source: &'a str) -> Cursor<'a> {
        Cursor {
            tokens,
            pos: 0,
            source,
            display_names: BTreeMap::new(),
            inserted_order: Vec::new(),
        }
    }

    /// `displayNames.putIfAbsent(name.toLowerCase(), name)` — the Java
    /// `AstBuilder` registration. Call it only where the Java visitor builds an
    /// `Expr.Var` / `Expr.ArrayAccess` from a user identifier.
    ///
    /// A speculative parse that is later rewound may have registered a name
    /// early. That is harmless: rewinding only ever re-parses the *same*
    /// tokens, so the re-parse re-registers the identical spelling and
    /// first-wins makes the second attempt a no-op.
    pub fn record_display_name(&mut self, original: &str) {
        let key = original.to_ascii_lowercase();
        if let std::collections::btree_map::Entry::Vacant(slot) =
            self.display_names.entry(key.clone())
        {
            slot.insert(original.to_string());
            self.inserted_order.push(key);
        }
    }

    /// A mark in the registration log, for [`Cursor::forget_display_name_if_new`].
    pub fn display_name_mark(&self) -> usize {
        self.inserted_order.len()
    }

    /// Undoes the registration of `original` **only if it was first inserted
    /// after `mark`**.
    ///
    /// The one caller is `parser::expr::parse_call_atom`: an argument that
    /// turns out to be a fluid/material/formula *token* is parsed as an
    /// expression first, which registers it like a variable, but the Java
    /// grammar has a dedicated token rule there so `AstBuilder.visitVarAtom`
    /// never sees it and `ParseResult.displayNames` gets no entry. The `mark`
    /// guard preserves the Java's first-wins semantics: a document that used
    /// `Steel` as a variable *before* writing `k_(Steel)` keeps the variable's
    /// spelling.
    pub fn forget_display_name_if_new(&mut self, original: &str, mark: usize) {
        let key = original.to_ascii_lowercase();
        let Some(at) = self.inserted_order[mark..].iter().position(|k| *k == key) else {
            return;
        };
        self.inserted_order.remove(mark + at);
        self.display_names.remove(&key);
    }

    /// The accumulated map, consumed at the end of `parse_document`.
    pub fn take_display_names(&mut self) -> BTreeMap<String, String> {
        std::mem::take(&mut self.display_names)
    }

    /// The original document text, for slicing `source_text` onto equations.
    pub fn source(&self) -> &'a str {
        self.source
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Rewind to a previously recorded position — used to back out of a
    /// speculative parse.
    pub fn reset_to(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// The current token kind, or `Eof` past the end.
    pub fn peek(&self) -> &TokenKind {
        self.peek_at(0)
    }

    /// The token kind `n` positions ahead, or `Eof` past the end.
    pub fn peek_at(&self, n: usize) -> &TokenKind {
        static EOF: TokenKind = TokenKind::Eof;
        self.tokens
            .get(self.pos + n)
            .map(|t| &t.kind)
            .unwrap_or(&EOF)
    }

    /// Span of the current token; a zero-width span at end of input.
    pub fn span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|t| t.span)
            .unwrap_or_else(|| Span::at(self.source.len()))
    }

    pub fn is_eof(&self) -> bool {
        matches!(self.peek(), TokenKind::Eof)
    }

    /// Consume and return the current token.
    pub fn advance(&mut self) -> &'a Token {
        static EOF_TOKEN: Token = Token {
            kind: TokenKind::Eof,
            span: Span { start: 0, end: 0 },
        };
        let tok = self.tokens.get(self.pos).unwrap_or(&EOF_TOKEN);
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    /// Consume the current token if it matches `kind`, reporting whether it did.
    pub fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.peek() == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Consume the current token, or fail with "expected X, found Y".
    pub fn expect(&mut self, kind: &TokenKind) -> Result<&'a Token> {
        if self.peek() == kind {
            Ok(self.advance())
        } else {
            Err(FreesError::parse_at(
                format!(
                    "expected {}, found {}",
                    kind.describe(),
                    self.peek().describe()
                ),
                self.span(),
            ))
        }
    }

    /// Consume an identifier, returning its text in the original case.
    pub fn expect_ident(&mut self) -> Result<String> {
        match self.peek().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(FreesError::parse_at(
                format!("expected an identifier, found {}", other.describe()),
                self.span(),
            )),
        }
    }

    /// Skip a run of statement separators (`;` and newlines). Returns true if
    /// at least one was consumed.
    pub fn skip_separators(&mut self) -> bool {
        let mut any = false;
        while self.peek().is_separator() {
            self.advance();
            any = true;
        }
        any
    }

    /// Skip newlines only — used inside bracketed constructs where a line
    /// break is not a statement boundary.
    pub fn skip_newlines(&mut self) {
        while matches!(self.peek(), TokenKind::Newline) {
            self.advance();
        }
    }
}

/// Error used for grammar the wasm port has not implemented yet.
pub fn unsupported(construct: &str, span: Span) -> FreesError {
    FreesError::parse_at(
        format!("`{construct}` blocks are not supported by the wasm engine yet"),
        span,
    )
}
