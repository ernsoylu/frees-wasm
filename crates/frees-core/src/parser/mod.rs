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

pub mod expr;
pub mod toplevel;

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
/// Block constructs that the wasm port has not reached yet (`FUNCTION`,
/// `PROCEDURE`, `MODULE`, `TABLE`, `PARAMETRIC`, `PLOT`, `STATE TABLE`,
/// `DYNAMIC`, `COMPONENT`, `connect`, `LINEARIZE`) are reported as an explicit
/// unsupported-construct error rather than being silently skipped — a wrong
/// answer is worse than a refusal.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub statements: Vec<Statement>,
    pub guesses: Vec<GuessDirective>,
    pub diagnostics: Vec<Diagnostic>,
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
}

impl<'a> Cursor<'a> {
    pub fn new(tokens: &'a [Token], source: &'a str) -> Cursor<'a> {
        Cursor {
            tokens,
            pos: 0,
            source,
        }
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
