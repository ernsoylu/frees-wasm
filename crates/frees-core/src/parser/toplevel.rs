//! Statement and top-level grammar.
//!
//! Rules `program`, `topLevel`, `statement`, `statementList`, `forBlock`,
//! `callStatement`, `multiAssign`, `rangeAssign`, `symbolicDecl`,
//! `guessDirective`, `equation` from `Frees.g4`, plus the definition blocks
//! `functionDef`, `procedureDef`, `moduleDef`, `tableDef`, the component rules
//! `componentDef`, `componentItem`, `componentVariant`, `componentParam`,
//! `componentInst`, `componentArgList`, `componentArg`, `connectStmt`,
//! `connectPort`, and the procedural body rules `procBody`, `procStatement`,
//! `assignment`, `ifStatement`, `repeatStatement`, `whileStatement`.
//!
//! Port of `AstBuilder.buildProgram` / `buildStatement` and friends
//! (`../frEES/backend/core/src/main/java/com/frees/backend/parser/AstBuilder.java`).
//!
//! # What this pass covers
//!
//! `topLevel` in the grammar admits eleven block constructs on top of the plain
//! statement forms. `guessDirective`, `statement`, the four definition blocks
//! (`FUNCTION` / `PROCEDURE` / `MODULE` / `TABLE`, filling [`Document::defs`])
//! and the three component forms (`componentDef` / `componentInst` /
//! `connectStmt`, filling [`Document::components`]) are implemented here; every
//! other leading token produces [`crate::parser::unsupported`] naming the
//! construct, so a document that uses one fails loudly instead of being
//! silently mis-parsed (`Document`'s doc comment: "a wrong answer is worse than
//! a refusal").
//!
//! # The component layer parses; it does not yet run
//!
//! Filling [`Document::components`] is a *grammar* milestone. Expansion —
//! cloning a body per instance, rewriting ports to stream variables, tying a
//! `connect` node together — is `ComponentExpander`'s job and is a separate
//! pass. Until it lands, [`crate::engine`] refuses a document that declared any
//! component rather than quietly solving the equations that remain, which is
//! the same "loud, not silent" rule the unsupported list enforces.
//!
//! # Multi-output `FUNCTION` desugaring
//!
//! The grammar allows `FUNCTION [a, b] = f(x) … END`, but the Java
//! `ProcDef.FunctionDef` record carries no outputs list. Verified against
//! `AstBuilder.buildFunctionDef`: when `funcOutputs` is present the block is
//! lowered to a **`ProcedureDef`** whose inputs are the parameter list and
//! whose outputs are the bracketed names — it reuses the procedure
//! call/flatten machinery and is consumed with `[p, q] = f(x)` (`multiAssign`,
//! itself sugar for `CALL f(x : p, q)`). This port mirrors that exactly.
//!
//! # Definition name collisions
//!
//! Java stores every definition in one `LinkedHashMap` keyed by the lowercase
//! name (`AstBuilder.buildProgram`), so a later definition of a name replaces
//! an earlier one **across kinds** (a `MODULE f` shadows an earlier
//! `FUNCTION f`). [`record_def`] mirrors that by evicting the name from all
//! four [`Definitions`] lists before inserting. (Java's `LinkedHashMap.put`
//! keeps the original insertion *slot* while this port re-appends; nothing
//! consumes the relative order of defs, only name lookups.)
//!
//! # Disambiguation
//!
//! Three decisions need more than one token of lookahead. All three are made by
//! pure prediction over [`Cursor::peek_at`] — there is no backtracking, so the
//! error reported on a malformed statement is always the one from the form the
//! source actually committed to.
//!
//! * **`multiAssign` vs. a matrix-literal equation.** `[a, ~, c] = f(x)` and
//!   `[a, b] = [1, 2]` start identically. The `at_multi_assign` predicate scans the
//!   whole candidate — output list, `=`, callee name, and the *balanced* argument
//!   list — and only commits when the token after the closing `)` ends the
//!   statement. `[a, b] = f(x) + 1` therefore parses as an ordinary equation,
//!   which is what ANTLR's adaptive prediction does with the same input.
//! * **`rangeAssign` vs. an equation.** `IDENT = signedNumber COLON …` is the
//!   discriminating prefix; a `:` cannot otherwise appear at the top of an
//!   expression, so the three-token lookahead is exact.
//! * **`componentInst`** is `IDENT IDENT (` — a shape the expression grammar
//!   cannot produce, so the three-token test is exact. It is predicted at
//!   `topLevel` and inside a `componentItem`, the only two places the grammar
//!   admits it: `statementList` (a `FOR` or `MODULE` body) does **not** list
//!   `componentInst`, so `Pump P1(a, b)` inside a `FOR` stays the syntax error
//!   ANTLR reports there.
//! * **`componentArg`** is `IDENT EQ expr` (named) or `expr` (positional). An
//!   `expr` can never contain a top-level `=`, so the two-token test is exact.
//!
//! # Range lowering
//!
//! `x = 0:10:100 | Log` becomes an [`Equation`] whose right-hand side is a call
//! to the **`range` intrinsic**:
//!
//! ```text
//! x = range(<start>, <middle>, <stop>, '<spacing>')
//! ```
//!
//! with `<middle>` the step for `linear` spacing and the point count for `log`,
//! `<spacing>` always present and lowercased (`'linear'` when no `| …` flag was
//! written), and the two-number form `a:b` normalised to `range(a, 1, b,
//! 'linear')`. The Java builder instead materialises the elements at parse time
//! into `x[1:N] = [v1, v2, …]`; keeping the range symbolic leaves that expansion
//! to the array/flattening layer and keeps the AST proportional to the source.
//! The *validation* Java does at parse time is kept verbatim (zero step, step
//! pointing the wrong way, element-count ceiling, log-range preconditions), so a
//! typo like `x = 0:0.0000001:100` is still rejected where the user wrote it.

use crate::ast::{Equation, Expr, Statement};
use crate::components::def::{
    ComponentDef, ComponentInst, ConnectDecl, Param, ParamOverrides, Variant,
};
use crate::diag::{FreesError, Result, Span};
use crate::parser::defs::{
    Curve, Definitions, FunctionDef, FunctionTableDef, ModuleDef, ProcStatement, ProcedureDef,
};
use crate::parser::expr::{parse_bool_expr, parse_unit_annotation};
use crate::parser::{unsupported, Cursor, Document, GuessDirective};
use crate::token::{Token, TokenKind};
use crate::units::UnitRegistry;

/// Canonical prefix for the throwaway "sink" variables backing a discarded
/// (`~`) output slot of a destructuring call. Port of
/// `EquationParser.IGNORED_OUTPUT_PREFIX`. A leading `~` can never appear in a
/// user identifier (the `IDENT` lexer rule starts with `[a-zA-Z]`), so these
/// names are unforgeable and downstream layers filter them out of results.
pub const IGNORED_OUTPUT_PREFIX: &str = "~ignored~";

/// True when a canonical variable name is an internal ignored-output sink.
/// Port of `EquationParser.isIgnoredSink`.
pub fn is_ignored_sink(canonical_name: &str) -> bool {
    canonical_name.starts_with(IGNORED_OUTPUT_PREFIX)
}

/// The intrinsic a `rangeAssign` lowers to. See the module docs.
pub const RANGE_INTRINSIC: &str = "range";

/// Maximum elements a single range may generate, so a typo like
/// `x = 0:0.0000001:100` cannot explode the equation system.
/// Mirrors `AstBuilder.MAX_RANGE_ELEMENTS`.
const MAX_RANGE_ELEMENTS: i64 = 100_000;

/// Maximum nesting of block statements (`FOR` inside `FOR` inside …).
///
/// `for_block` calls `statement`, which calls `for_block` again, so a document
/// consisting of nothing but `FOR` headers recurses once per header and
/// overflows the stack — an abort no caller can catch. A hand-written document
/// nests a handful of loops; 64 is far beyond that and far below the ceiling.
/// Same reasoning as [`crate::parser::expr::MAX_EXPR_DEPTH`], which bounds the
/// expression half of the grammar.
const MAX_BLOCK_DEPTH: u32 = 64;

/// The expression sub-grammar, injected so the statement grammar can be
/// exercised on its own. Production always passes
/// [`crate::parser::expr::parse_expr`].
type ExprFn = fn(&mut Cursor<'_>) -> Result<Expr>;

/// Parse a whole document from source text.
///
/// Lexes, then parses. Unsupported block constructs produce an explicit error.
pub fn parse_document(source: &str) -> Result<Document> {
    let tokens = crate::lexer::tokenize(source)?;
    parse_token_stream(source, &tokens, crate::parser::expr::parse_expr)
}

/// The whole of [`parse_document`] minus the lexing step.
fn parse_token_stream<'a>(
    source: &'a str,
    tokens: &'a [Token],
    expr_fn: ExprFn,
) -> Result<Document> {
    let mut parser = Parser {
        c: Cursor::new(tokens, source),
        tokens,
        expr_fn,
        sinks: 0,
        block_depth: 0,
    };
    parser.program()
}

struct Parser<'a> {
    c: Cursor<'a>,
    /// The same slice the cursor walks, kept so a statement can be sliced back
    /// out of the source verbatim once its last token has been consumed.
    tokens: &'a [Token],
    expr_fn: ExprFn,
    /// Per-document counter for `~` sink names. The Java engine uses a
    /// process-global `AtomicLong`; a per-document counter is deterministic,
    /// which matters for wasm/native parity and for tests.
    sinks: u32,
    /// How many block statements are currently open, for [`MAX_BLOCK_DEPTH`].
    block_depth: u32,
}

impl<'a> Parser<'a> {
    // ── program / topLevel ──────────────────────────────────────────────────

    /// `program : sep? (topLevel (sep topLevel)* sep?)? EOF`
    fn program(&mut self) -> Result<Document> {
        let mut doc = Document::default();
        self.c.skip_separators();
        while !self.c.is_eof() {
            self.top_level(&mut doc)?;
            if self.c.is_eof() {
                break;
            }
            if !self.c.skip_separators() {
                return Err(FreesError::parse_at(
                    format!(
                        "expected end of statement, found {}",
                        self.c.peek().describe()
                    ),
                    self.c.span(),
                ));
            }
        }
        doc.display_names = self.c.take_display_names();
        Ok(doc)
    }

    /// `topLevel`. `guessDirective`, the four definition blocks, the three
    /// component forms and `statement` are supported; every other block form is
    /// rejected by [`Parser::statement`].
    fn top_level(&mut self, doc: &mut Document) -> Result<()> {
        match self.c.peek() {
            TokenKind::Guess => {
                let directive = self.guess_directive()?;
                doc.guesses.push(directive);
            }
            TokenKind::Component => {
                let def = self.component_def()?;
                doc.components.defs.push(def);
            }
            TokenKind::Connect => {
                let connect = self.connect_stmt()?;
                doc.components.connects.push(connect);
            }
            // `componentInst` before `statement`: the grammar lists it as its
            // own `topLevel` alternative, and `IDENT IDENT (` is a shape no
            // `equation` can take.
            TokenKind::Ident(_) if self.at_component_inst() => {
                let inst = self.component_inst()?;
                doc.components.instances.push(inst);
            }
            TokenKind::Function => {
                let def = self.function_def()?;
                record_def(&mut doc.defs, def);
            }
            TokenKind::Procedure => {
                let def = self.procedure_def()?;
                record_def(&mut doc.defs, ParsedDef::Procedure(def));
            }
            TokenKind::Module => {
                let def = self.module_def()?;
                record_def(&mut doc.defs, ParsedDef::Module(def));
            }
            TokenKind::Table => {
                let def = self.table_def()?;
                record_def(&mut doc.defs, ParsedDef::Table(def));
            }
            _ => {
                let statement = self.statement()?;
                doc.statements.push(statement);
            }
        }
        Ok(())
    }

    // ── statement ───────────────────────────────────────────────────────────

    /// `statement : forBlock | callStatement | symbolicDecl | multiAssign
    ///            | rangeAssign | equation`
    fn statement(&mut self) -> Result<Statement> {
        if let Some(construct) = unsupported_construct(self.c.peek()) {
            return Err(unsupported(construct, self.c.span()));
        }
        match self.c.peek().clone() {
            TokenKind::For => self.for_block(),
            TokenKind::Call => self.call_statement(),
            TokenKind::Symbolic => self.symbolic_decl(),
            TokenKind::End => Err(FreesError::parse_at(
                "unexpected `END` — no block is open here",
                self.c.span(),
            )),
            TokenKind::LBracket if self.at_multi_assign() => self.multi_assign(),
            TokenKind::Ident(_) if self.at_range_assign() => self.range_assign(),
            _ => self.equation(),
        }
    }

    /// `forBlock : FOR IDENT EQ expr TO expr sep statementList sep? END`
    fn for_block(&mut self) -> Result<Statement> {
        let header = self.c.span();
        self.enter_block(header)?;
        let result = self.for_block_inner(header);
        self.block_depth -= 1;
        result
    }

    fn for_block_inner(&mut self, header: Span) -> Result<Statement> {
        self.c.expect(&TokenKind::For)?;
        let var_name = self.c.expect_ident()?.to_ascii_lowercase();
        self.c.expect(&TokenKind::Eq)?;
        let start = self.expr()?;
        self.c.expect(&TokenKind::To)?;
        let end = self.expr()?;
        self.require_sep("after the FOR header")?;

        // `statementList sep? END`
        let body = self.statement_list_until_end(header, "FOR")?;
        self.c.expect(&TokenKind::End)?;

        Ok(Statement::For {
            var_name,
            start,
            end,
            body,
        })
    }

    /// `statementList sep? END` — the shared body shape of `forBlock` and
    /// `moduleDef`. Stops **at** the `END` without consuming it.
    fn statement_list_until_end(&mut self, header: Span, block: &str) -> Result<Vec<Statement>> {
        let mut body = Vec::new();
        loop {
            self.c.skip_separators();
            if matches!(self.c.peek(), TokenKind::End) {
                break;
            }
            if self.c.is_eof() {
                return Err(FreesError::parse_at(
                    format!("unterminated {block} block: expected `END`"),
                    header,
                ));
            }
            body.push(self.statement()?);
            if !self.c.peek().is_separator()
                && !matches!(self.c.peek(), TokenKind::End)
                && !self.c.is_eof()
            {
                return Err(FreesError::parse_at(
                    format!(
                        "expected end of statement, found {}",
                        self.c.peek().describe()
                    ),
                    self.c.span(),
                ));
            }
        }
        Ok(body)
    }

    /// `callStatement : CALL IDENT LPAREN callArgList COLON callArgList RPAREN`
    fn call_statement(&mut self) -> Result<Statement> {
        let start_pos = self.c.pos();
        self.c.expect(&TokenKind::Call)?;
        let name = self.c.expect_ident()?.to_ascii_lowercase();
        self.c.expect(&TokenKind::LParen)?;
        let inputs = self.call_arg_list(&TokenKind::Colon)?;
        self.c.expect(&TokenKind::Colon)?;
        let outputs = self.call_arg_list(&TokenKind::RParen)?;
        self.c.expect(&TokenKind::RParen)?;
        Ok(Statement::CallProc {
            name,
            inputs,
            outputs,
            source_text: self.text_since(start_pos),
        })
    }

    /// `callArgList : (expr (COMMA expr)*)?` — `terminator` is the token that
    /// closes the list, so an empty list is recognised without lookahead.
    fn call_arg_list(&mut self, terminator: &TokenKind) -> Result<Vec<Expr>> {
        let mut args = Vec::new();
        // A line break inside a bracketed construct is not a statement boundary.
        self.c.skip_newlines();
        if self.c.peek() == terminator {
            return Ok(args);
        }
        loop {
            args.push(self.expr()?);
            self.c.skip_newlines();
            if self.c.eat(&TokenKind::Comma) {
                self.c.skip_newlines();
                continue;
            }
            break;
        }
        Ok(args)
    }

    /// `multiAssign : LBRACKET callOutputs RBRACKET EQ IDENT LPAREN callArgList RPAREN`
    ///
    /// Sugar for `CALL f(inputs : outputs)`; lowered to exactly the
    /// [`Statement::CallProc`] the explicit form produces, so procedure
    /// flattening handles both unchanged. A `~` output slot binds a fresh
    /// unforgeable sink variable (see [`IGNORED_OUTPUT_PREFIX`]) — the solver
    /// still computes that output, it is just never surfaced.
    fn multi_assign(&mut self) -> Result<Statement> {
        let start_pos = self.c.pos();
        self.c.expect(&TokenKind::LBracket)?;
        let mut outputs = Vec::new();
        loop {
            self.c.skip_newlines();
            match self.c.peek().clone() {
                TokenKind::Tilde => {
                    self.c.advance();
                    outputs.push(self.new_ignored_sink());
                }
                TokenKind::Ident(name) => {
                    self.c.advance();
                    outputs.push(Expr::var(name));
                }
                other => {
                    return Err(FreesError::parse_at(
                        format!("expected an output name or `~`, found {}", other.describe()),
                        self.c.span(),
                    ))
                }
            }
            self.c.skip_newlines();
            if self.c.eat(&TokenKind::Comma) {
                continue;
            }
            break;
        }
        self.c.expect(&TokenKind::RBracket)?;
        self.c.expect(&TokenKind::Eq)?;
        let name = self.c.expect_ident()?.to_ascii_lowercase();
        self.c.expect(&TokenKind::LParen)?;
        let inputs = self.call_arg_list(&TokenKind::RParen)?;
        self.c.expect(&TokenKind::RParen)?;
        Ok(Statement::CallProc {
            name,
            inputs,
            outputs,
            source_text: self.text_since(start_pos),
        })
    }

    /// `symbolicDecl : SYMBOLIC IDENT (COMMA IDENT)*`
    fn symbolic_decl(&mut self) -> Result<Statement> {
        self.c.expect(&TokenKind::Symbolic)?;
        let mut names = vec![self.c.expect_ident()?.to_ascii_lowercase()];
        while self.c.eat(&TokenKind::Comma) {
            names.push(self.c.expect_ident()?.to_ascii_lowercase());
        }
        Ok(Statement::Symbolic(names))
    }

    /// `rangeAssign : IDENT EQ signedNumber COLON signedNumber
    ///                (COLON signedNumber)? (PIPE IDENT)?`
    ///
    /// See the module docs for the lowering.
    fn range_assign(&mut self) -> Result<Statement> {
        let start_pos = self.c.pos();
        let name = self.c.expect_ident()?.to_ascii_lowercase();
        self.c.expect(&TokenKind::Eq)?;
        let first = self.signed_number()?;
        self.c.expect(&TokenKind::Colon)?;
        let second = self.signed_number()?;
        let third = if self.c.eat(&TokenKind::Colon) {
            Some(self.signed_number()?)
        } else {
            None
        };
        let spacing_written = if self.c.eat(&TokenKind::Pipe) {
            self.c.expect_ident()?
        } else {
            "Linear".to_string()
        };

        let three_form = third.is_some();
        let start = first;
        let stop = third.unwrap_or(second);
        // 2-number form (start:stop) implies step 1; the 3-number form gives
        // the step (linear) or the point count (log).
        let middle = if three_form { second } else { 1.0 };
        let spacing = spacing_written.to_ascii_lowercase();
        let span = self.span_since(start_pos);

        match spacing.as_str() {
            "linear" => {
                linear_range_count(&name, start, middle, stop, span)?;
            }
            "log" => {
                log_range_count(&name, start, middle, stop, three_form, span)?;
            }
            _ => {
                return Err(FreesError::parse_at(
                    format!(
                        "Unknown range spacing '{spacing_written}' in {name} = ... \
                         Supported: Linear, Log."
                    ),
                    span,
                ))
            }
        }

        let rhs = Expr::call(
            RANGE_INTRINSIC,
            vec![
                Expr::num(start),
                Expr::num(middle),
                Expr::num(stop),
                Expr::Str(spacing),
            ],
        );
        Ok(Statement::Eq(Equation::new(
            Expr::var(&name),
            rhs,
            self.text_since(start_pos),
        )))
    }

    /// `equation : expr EQ expr`
    fn equation(&mut self) -> Result<Statement> {
        Ok(Statement::Eq(self.bare_equation()?))
    }

    /// `equation : expr EQ expr`, as the bare [`Equation`] the component AST
    /// stores. [`Parser::equation`] wraps it into a [`Statement`].
    fn bare_equation(&mut self) -> Result<Equation> {
        let start_pos = self.c.pos();
        let lhs = self.expr()?;
        self.c.expect(&TokenKind::Eq)?;
        let rhs = self.expr()?;
        Ok(Equation::new(lhs, rhs, self.text_since(start_pos)))
    }

    // ── guessDirective ──────────────────────────────────────────────────────

    /// `guessDirective : GUESS IDENT (EQ signedNumber)?
    ///                   (LBRACKET signedNumber COMMA signedNumber RBRACKET)?`
    ///
    /// Port of `AstBuilder.buildGuessDirective`, including its three rejections:
    /// crossed bounds, a directive that declares neither a guess nor bounds, and
    /// a guess outside its own bounds.
    fn guess_directive(&mut self) -> Result<GuessDirective> {
        let start_pos = self.c.pos();
        self.c.expect(&TokenKind::Guess)?;
        let name = self.c.expect_ident()?.to_ascii_lowercase();

        let guess = if self.c.eat(&TokenKind::Eq) {
            Some(self.signed_number()?)
        } else {
            None
        };

        let mut lower = None;
        let mut upper = None;
        if self.c.eat(&TokenKind::LBracket) {
            let lo = self.signed_number()?;
            self.c.expect(&TokenKind::Comma)?;
            let hi = self.signed_number()?;
            self.c.expect(&TokenKind::RBracket)?;
            if lo >= hi {
                return Err(FreesError::parse_at(
                    format!("GUESS {name}: the lower bound must be below the upper bound."),
                    self.span_since(start_pos),
                ));
            }
            lower = Some(lo);
            upper = Some(hi);
        }

        if guess.is_none() && lower.is_none() {
            return Err(FreesError::parse_at(
                format!(
                    "GUESS {name}: declare a guess (GUESS {name} = 2), \
                     bounds (GUESS {name} [0, 10]), or both."
                ),
                self.span_since(start_pos),
            ));
        }
        if let (Some(g), Some(lo), Some(hi)) = (guess, lower, upper) {
            if g < lo || g > hi {
                return Err(FreesError::parse_at(
                    format!("GUESS {name}: the guess {g} lies outside [{lo}, {hi}]."),
                    self.span_since(start_pos),
                ));
            }
        }

        Ok(GuessDirective {
            name,
            guess,
            lower,
            upper,
        })
    }

    // ── FUNCTION / PROCEDURE / MODULE / TABLE definitions ───────────────────

    /// `functionDef : FUNCTION (LBRACKET funcOutputs RBRACKET EQ)? IDENT
    ///                LPAREN paramList RPAREN unit? sep procBody END`
    ///
    /// Port of `AstBuilder.buildFunctionDef`. The multi-output form is lowered
    /// to a [`ProcedureDef`] (see the module docs); its unit annotations are
    /// parsed and discarded exactly as the Java builder ignores them.
    fn function_def(&mut self) -> Result<ParsedDef> {
        let header = self.c.span();
        self.c.expect(&TokenKind::Function)?;

        // `(LBRACKET funcOutputs RBRACKET EQ)?` — the multi-output header.
        let outputs = if self.c.eat(&TokenKind::LBracket) {
            let mut outs = vec![self.c.expect_ident()?.to_ascii_lowercase()];
            while self.c.eat(&TokenKind::Comma) {
                outs.push(self.c.expect_ident()?.to_ascii_lowercase());
            }
            self.c.expect(&TokenKind::RBracket)?;
            self.c.expect(&TokenKind::Eq)?;
            Some(outs)
        } else {
            None
        };

        let name = self.c.expect_ident()?.to_ascii_lowercase();
        self.c.expect(&TokenKind::LParen)?;
        let (params, param_units) = self.param_list()?;
        self.c.expect(&TokenKind::RParen)?;
        let output_unit = si_unit_of(parse_unit_annotation(&mut self.c)?);
        self.require_sep("after the FUNCTION header")?;
        let body = self.proc_body(header, "FUNCTION", &[TokenKind::End])?;
        self.c.expect(&TokenKind::End)?;

        Ok(match outputs {
            // `FUNCTION [a, b] = f(x)` → ProcedureDef (AstBuilder parity).
            Some(outputs) => ParsedDef::Procedure(ProcedureDef {
                name,
                inputs: params,
                outputs,
                body,
            }),
            None => ParsedDef::Function(FunctionDef {
                name,
                params,
                body,
                output_unit,
                param_units: collapse_units(param_units),
            }),
        })
    }

    /// `procedureDef : PROCEDURE IDENT LPAREN paramList COLON paramList RPAREN
    ///                 sep procBody END`
    ///
    /// Port of `AstBuilder.buildProcedureDef` — parameter units are parsed and
    /// discarded (`buildParamList` keeps names only).
    fn procedure_def(&mut self) -> Result<ProcedureDef> {
        let header = self.c.span();
        self.c.expect(&TokenKind::Procedure)?;
        let name = self.c.expect_ident()?.to_ascii_lowercase();
        self.c.expect(&TokenKind::LParen)?;
        let (inputs, _) = self.param_list()?;
        self.c.expect(&TokenKind::Colon)?;
        let (outputs, _) = self.param_list()?;
        self.c.expect(&TokenKind::RParen)?;
        self.require_sep("after the PROCEDURE header")?;
        let body = self.proc_body(header, "PROCEDURE", &[TokenKind::End])?;
        self.c.expect(&TokenKind::End)?;
        Ok(ProcedureDef {
            name,
            inputs,
            outputs,
            body,
        })
    }

    /// `moduleDef : MODULE IDENT LPAREN paramList COLON paramList RPAREN sep
    ///              statementList sep? END`
    ///
    /// Port of `AstBuilder.buildModuleDef`. The body is a **statement** list
    /// (`=` equations, FOR, …), not a procedural body — flattening grafts the
    /// equations into the caller's system with namespaced variable names.
    fn module_def(&mut self) -> Result<ModuleDef> {
        let header = self.c.span();
        self.c.expect(&TokenKind::Module)?;
        let name = self.c.expect_ident()?.to_ascii_lowercase();
        self.c.expect(&TokenKind::LParen)?;
        let (inputs, _) = self.param_list()?;
        self.c.expect(&TokenKind::Colon)?;
        let (outputs, _) = self.param_list()?;
        self.c.expect(&TokenKind::RParen)?;
        self.require_sep("after the MODULE header")?;
        let body = self.statement_list_until_end(header, "MODULE")?;
        self.c.expect(&TokenKind::End)?;
        Ok(ModuleDef {
            name,
            inputs,
            outputs,
            body,
        })
    }

    /// `tableDef : TABLE IDENT LPAREN IDENT unit? (COLON IDENT EQ numberList)?
    ///             RPAREN unit? tableFlags? sep tableRow (sep tableRow)* sep?
    ///             END`
    ///
    /// Port of `AstBuilder.buildTableDef`: the first body column is the lookup
    /// argument, each further column one curve; a family declares its
    /// parameter values in the header. Curves are sorted ascending by x
    /// exactly as `buildCurves` does, and ragged rows simply omit later
    /// columns.
    fn table_def(&mut self) -> Result<FunctionTableDef> {
        let header = self.c.span();
        self.c.expect(&TokenKind::Table)?;
        let name = self.c.expect_ident()?.to_ascii_lowercase();
        self.c.expect(&TokenKind::LParen)?;
        let arg_name = self.c.expect_ident()?.to_ascii_lowercase();
        let arg_unit = si_unit_of(parse_unit_annotation(&mut self.c)?);

        // `(COLON IDENT EQ numberList)?` — the curve-family header.
        let family = if self.c.eat(&TokenKind::Colon) {
            let param_name = self.c.expect_ident()?.to_ascii_lowercase();
            self.c.expect(&TokenKind::Eq)?;
            let mut values = vec![self.signed_number()?];
            while self.c.eat(&TokenKind::Comma) {
                values.push(self.signed_number()?);
            }
            Some((param_name, values))
        } else {
            None
        };
        self.c.expect(&TokenKind::RParen)?;
        let output_unit = si_unit_of(parse_unit_annotation(&mut self.c)?);

        // `tableFlags : IDENT+` — XLOG/LOGX, YLOG/LOGY (parseTableFlags).
        let mut x_log = false;
        let mut y_log = false;
        while let TokenKind::Ident(flag) = self.c.peek().clone() {
            let flag_span = self.c.span();
            self.c.advance();
            match flag.to_ascii_lowercase().as_str() {
                "xlog" | "logx" => x_log = true,
                "ylog" | "logy" => y_log = true,
                _ => {
                    return Err(FreesError::parse_at(
                        format!(
                            "Unknown TABLE flag '{flag}' in {name}(...). \
                             Supported flags: XLOG, YLOG."
                        ),
                        flag_span,
                    ))
                }
            }
        }
        self.require_sep("after the TABLE header")?;

        // `tableRow (sep tableRow)* sep?` — rows of whitespace-separated
        // signed numbers. At least one row, as the grammar requires; an
        // immediate `END` falls into `signed_number` and earns the natural
        // "expected a number, found `END`".
        let mut rows: Vec<Vec<f64>> = Vec::new();
        loop {
            self.c.skip_separators();
            if matches!(self.c.peek(), TokenKind::End) && !rows.is_empty() {
                break;
            }
            if self.c.is_eof() {
                return Err(FreesError::parse_at(
                    "unterminated TABLE block: expected `END`",
                    header,
                ));
            }
            let mut row = vec![self.signed_number()?];
            while matches!(
                self.c.peek(),
                TokenKind::Number { .. } | TokenKind::Plus | TokenKind::Minus
            ) {
                row.push(self.signed_number()?);
            }
            rows.push(row);
            if !self.c.peek().is_separator() && !matches!(self.c.peek(), TokenKind::End) {
                return Err(FreesError::parse_at(
                    format!(
                        "expected end of statement, found {}",
                        self.c.peek().describe()
                    ),
                    self.c.span(),
                ));
            }
        }
        self.c.expect(&TokenKind::End)?;

        // Read every row as [x, y1, y2, ...]; ragged rows omit later columns.
        let max_cols = rows.iter().map(Vec::len).max().unwrap_or(0);
        let curve_count = max_cols.saturating_sub(1).max(1);
        if let Some((_, values)) = &family {
            if values.len() != curve_count {
                return Err(FreesError::parse_at(
                    format!(
                        "TABLE {name}: header declares {} curve parameter value(s) \
                         but the rows have {curve_count} value column(s).",
                        values.len()
                    ),
                    header,
                ));
            }
        }

        // One Curve per value column, each sorted ascending by x
        // (`buildCurves`; Java's `Double.compare` order — `total_cmp` agrees on
        // -0.0 < 0.0 and NaN-greatest).
        let mut curves = Vec::with_capacity(curve_count);
        for j in 0..curve_count {
            let mut pts: Vec<(f64, f64)> = rows
                .iter()
                .filter(|row| row.len() >= j + 2)
                .map(|row| (row[0], row[j + 1]))
                .collect();
            pts.sort_by(|a, b| a.0.total_cmp(&b.0));
            curves.push(Curve {
                param: family.as_ref().map(|(_, values)| values[j]),
                xs: pts.iter().map(|p| p.0).collect(),
                ys: pts.iter().map(|p| p.1).collect(),
            });
        }

        let mut arg_names = vec![arg_name];
        let mut arg_units = vec![arg_unit];
        if let Some((param_name, _)) = &family {
            arg_names.push(param_name.clone());
            arg_units.push(None); // family-parameter units are not annotated yet
        }
        Ok(FunctionTableDef {
            name,
            arg_names,
            x_log,
            y_log,
            curves,
            output_unit,
            arg_units: collapse_units(arg_units),
        })
    }

    /// `paramList : (IDENT unit? (COMMA IDENT unit?)*)?` — names lowercased,
    /// units converted to their SI display names aligned with the names
    /// (`AstBuilder.buildParamList` + `paramUnits`).
    fn param_list(&mut self) -> Result<(Vec<String>, Vec<Option<String>>)> {
        let mut names = Vec::new();
        let mut units = Vec::new();
        // The list is optional; `)` or `:` closes an empty one.
        if matches!(self.c.peek(), TokenKind::RParen | TokenKind::Colon) {
            return Ok((names, units));
        }
        loop {
            names.push(self.c.expect_ident()?.to_ascii_lowercase());
            units.push(si_unit_of(parse_unit_annotation(&mut self.c)?));
            if self.c.eat(&TokenKind::Comma) {
                continue;
            }
            break;
        }
        Ok((names, units))
    }

    // ── COMPONENT / instantiation / connect ─────────────────────────────────

    /// `componentDef : COMPONENT IDENT LPAREN paramList RPAREN sep
    ///                 componentItem (sep componentItem)* sep? END`
    ///
    /// Port of `AstBuilder.buildComponentDef`. Ports come from the shared
    /// `paramList` rule, so a port may legally carry a unit annotation that
    /// `buildParamList` then drops. The `REQUIRE`-to-parameter promotion the
    /// Java does after the walk lives in [`ComponentDef::new`].
    fn component_def(&mut self) -> Result<ComponentDef> {
        let header = self.c.span();
        self.c.expect(&TokenKind::Component)?;
        let name = self.c.expect_ident()?.to_ascii_lowercase();
        self.c.expect(&TokenKind::LParen)?;
        let (ports, _) = self.param_list()?;
        self.c.expect(&TokenKind::RParen)?;
        self.require_sep("after the COMPONENT header")?;

        let mut params = Vec::new();
        let mut body = Vec::new();
        let mut variants = Vec::new();
        let mut sub_instances = Vec::new();
        let mut sub_connects = Vec::new();

        // `componentItem (sep componentItem)* sep? END`. The grammar demands at
        // least one item, so an immediate `END` is not a break — it falls into
        // `bare_equation` and earns the natural "found `END`", the same way
        // `table_def` handles an empty body.
        let mut items = 0usize;
        loop {
            self.c.skip_separators();
            if items > 0 && matches!(self.c.peek(), TokenKind::End) {
                break;
            }
            if self.c.is_eof() {
                return Err(FreesError::parse_at(
                    format!("unterminated COMPONENT {name} block: expected `END`"),
                    header,
                ));
            }
            // `componentItem : PARAM … | componentVariant | componentInst
            //                | connectStmt | equation`
            if matches!(self.c.peek(), TokenKind::Param) {
                params.extend(self.component_param_line()?);
            } else if matches!(self.c.peek(), TokenKind::Variant) {
                variants.push(self.component_variant()?);
            } else if matches!(self.c.peek(), TokenKind::Connect) {
                sub_connects.push(self.connect_stmt()?);
            } else if self.at_component_inst() {
                sub_instances.push(self.component_inst()?);
            } else {
                body.push(self.bare_equation()?);
            }
            items += 1;
            if !self.c.peek().is_separator()
                && !matches!(self.c.peek(), TokenKind::End)
                && !self.c.is_eof()
            {
                return Err(FreesError::parse_at(
                    format!(
                        "expected end of statement, found {}",
                        self.c.peek().describe()
                    ),
                    self.c.span(),
                ));
            }
        }
        self.c.expect(&TokenKind::End)?;

        Ok(ComponentDef::new(
            name,
            ports,
            params,
            body,
            variants,
            sub_instances,
            sub_connects,
        ))
    }

    /// `PARAM componentParam (COMMA componentParam)*` — one `PARAM` line, which
    /// may declare several parameters.
    fn component_param_line(&mut self) -> Result<Vec<Param>> {
        self.c.expect(&TokenKind::Param)?;
        let mut params = vec![self.component_param()?];
        while self.c.eat(&TokenKind::Comma) {
            params.push(self.component_param()?);
        }
        Ok(params)
    }

    /// `componentParam : IDENT (EQ expr)?`
    ///
    /// The default is optional in the *language*; the standard library
    /// deliberately supplies none for a physical input, and a parameter left
    /// without one is refused at expansion time, not here — see
    /// [`crate::components::def`]'s module docs.
    fn component_param(&mut self) -> Result<Param> {
        let name = self.c.expect_ident()?.to_ascii_lowercase();
        let default_value = if self.c.eat(&TokenKind::Eq) {
            Some(self.expr()?)
        } else {
            None
        };
        Ok(Param::new(name, default_value))
    }

    /// `componentVariant : VARIANT IDENT (REQUIRE IDENT (COMMA IDENT)*)? sep
    ///                     (equation sep)* END`
    ///
    /// Port of `AstBuilder.buildComponentVariant`. A variant body is equations
    /// only — no `PARAM`, no nested `VARIANT`, no sub-instance.
    fn component_variant(&mut self) -> Result<Variant> {
        let header = self.c.span();
        self.c.expect(&TokenKind::Variant)?;
        let name = self.c.expect_ident()?.to_ascii_lowercase();

        let mut require = Vec::new();
        if self.c.eat(&TokenKind::Require) {
            require.push(self.c.expect_ident()?.to_ascii_lowercase());
            while self.c.eat(&TokenKind::Comma) {
                require.push(self.c.expect_ident()?.to_ascii_lowercase());
            }
        }
        self.require_sep("after the VARIANT header")?;

        let mut body = Vec::new();
        loop {
            self.c.skip_separators();
            if matches!(self.c.peek(), TokenKind::End) {
                break;
            }
            if self.c.is_eof() {
                return Err(FreesError::parse_at(
                    format!("unterminated VARIANT {name} block: expected `END`"),
                    header,
                ));
            }
            body.push(self.bare_equation()?);
            // `(equation sep)*`: unlike `componentDef`'s trailing `sep?`, the
            // variant rule spells the separator after *every* equation, so
            // `x = 1 END` on one line is not a well-formed variant body.
            self.require_sep("after a VARIANT equation")?;
        }
        self.c.expect(&TokenKind::End)?;

        Ok(Variant {
            name,
            require,
            body,
        })
    }

    /// `componentInst : IDENT IDENT LPAREN componentArgList RPAREN`
    ///
    /// Port of `AstBuilder.buildComponentInst`: leading positional arguments
    /// bind ports to stream names in declaration order, trailing `name=value`
    /// arguments override parameters. Both of the Java's rejections are kept.
    fn component_inst(&mut self) -> Result<ComponentInst> {
        let start_pos = self.c.pos();
        let type_name = self.c.expect_ident()?.to_ascii_lowercase();
        let name = self.c.expect_ident()?.to_ascii_lowercase();
        self.c.expect(&TokenKind::LParen)?;

        let mut port_args = Vec::new();
        let mut params = ParamOverrides::new();
        // `componentArgList : (componentArg (COMMA componentArg)*)?` — the list
        // is optional, as `TransGround G()` in the standard library relies on.
        if !matches!(self.c.peek(), TokenKind::RParen) {
            loop {
                self.component_arg(&name, &mut port_args, &mut params)?;
                if !self.c.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.c.expect(&TokenKind::RParen)?;

        Ok(ComponentInst {
            type_name,
            name,
            port_args,
            params,
            source_text: self.text_since(start_pos),
        })
    }

    /// `componentArg : IDENT EQ expr # CompArgNamed | expr # CompArgPos`
    ///
    /// An `expr` can never contain a top-level `=`, so `IDENT EQ` decides the
    /// alternative exactly. Ordering is checked before the positional argument
    /// is parsed, exactly where `buildComponentInst` checks it.
    fn component_arg(
        &mut self,
        inst_name: &str,
        port_args: &mut Vec<String>,
        params: &mut ParamOverrides,
    ) -> Result<()> {
        if matches!(self.c.peek(), TokenKind::Ident(_))
            && matches!(self.c.peek_at(1), TokenKind::Eq)
        {
            let name = self.c.expect_ident()?.to_ascii_lowercase();
            self.c.expect(&TokenKind::Eq)?;
            let value = self.expr()?;
            // `LinkedHashMap.put`: a repeated name overwrites in place.
            params.put(name, value);
            return Ok(());
        }

        if !params.is_empty() {
            return Err(FreesError::parse_at(
                format!(
                    "Component '{inst_name}': positional port arguments must come \
                     before name=value parameters."
                ),
                self.c.span(),
            ));
        }
        let start_pos = self.c.pos();
        let value = self.expr()?;
        // `Expr.Var` covers both a bare stream name and a dotted one; anything
        // else (a number, a call, an arithmetic expression) is not a stream.
        //
        // The quoted text is the user's, verbatim — the port's convention for
        // `source_text` everywhere. Java quotes ANTLR's `ctx.getText()`, which
        // concatenates tokens with the whitespace stripped, so the Java writes
        // `got: s1+1` where this writes `got: s1 + 1`. Verified against the
        // oracle; the rejection itself is identical.
        match value {
            Expr::Var(stream) => port_args.push(stream),
            _ => {
                return Err(FreesError::parse_at(
                    format!(
                    "Component '{inst_name}': each port argument must be a stream name, got: {}",
                    self.text_since(start_pos)
                ),
                    self.span_since(start_pos),
                ))
            }
        }
        Ok(())
    }

    /// `connectStmt : CONNECT LPAREN connectPort (COMMA connectPort)* RPAREN`
    ///
    /// Port of `AstBuilder.buildConnect`.
    fn connect_stmt(&mut self) -> Result<ConnectDecl> {
        let start_pos = self.c.pos();
        self.c.expect(&TokenKind::Connect)?;
        self.c.expect(&TokenKind::LParen)?;
        let mut ports = vec![self.connect_port()?];
        while self.c.eat(&TokenKind::Comma) {
            ports.push(self.connect_port()?);
        }
        self.c.expect(&TokenKind::RParen)?;
        Ok(ConnectDecl {
            ports,
            source_text: self.text_since(start_pos),
        })
    }

    /// `connectPort : IDENT (DOT IDENT)*` — an endpoint is a *name*, not an
    /// expression: `buildConnect` reads the `IDENT` tokens straight off the
    /// parse tree and joins them with `.`, so a connect endpoint never becomes
    /// an `Expr::Var` and never registers a display name.
    fn connect_port(&mut self) -> Result<String> {
        let mut path = self.c.expect_ident()?.to_ascii_lowercase();
        while self.c.eat(&TokenKind::Dot) {
            path.push('.');
            path.push_str(&self.c.expect_ident()?.to_ascii_lowercase());
        }
        Ok(path)
    }

    // ── procedural bodies (inside FUNCTION / PROCEDURE) ─────────────────────

    /// `procBody : (procStatement (sep procStatement)* sep?)?` — collects
    /// statements until one of `terminators` (`END`, `ELSE`, or `UNTIL`),
    /// which is left unconsumed.
    fn proc_body(
        &mut self,
        header: Span,
        block: &str,
        terminators: &[TokenKind],
    ) -> Result<Vec<ProcStatement>> {
        let mut body = Vec::new();
        loop {
            self.c.skip_separators();
            if terminators.contains(self.c.peek()) {
                break;
            }
            if self.c.is_eof() {
                let expected = match terminators {
                    [TokenKind::Until] => "`UNTIL`",
                    _ => "`END`",
                };
                return Err(FreesError::parse_at(
                    format!("unterminated {block} block: expected {expected}"),
                    header,
                ));
            }
            body.push(self.proc_statement()?);
            if !self.c.peek().is_separator()
                && !terminators.contains(self.c.peek())
                && !self.c.is_eof()
            {
                return Err(FreesError::parse_at(
                    format!(
                        "expected end of statement, found {}",
                        self.c.peek().describe()
                    ),
                    self.c.span(),
                ));
            }
        }
        Ok(body)
    }

    /// `procStatement : forBlock | ifStatement | repeatStatement
    ///                | whileStatement | assignment | equation`
    fn proc_statement(&mut self) -> Result<ProcStatement> {
        match self.c.peek() {
            TokenKind::If => self.if_statement(),
            TokenKind::Repeat => self.repeat_statement(),
            TokenKind::While => self.while_statement(),
            // The grammar reuses the top-level `forBlock` (whose body is a
            // *statement* list); `buildProcFor` re-parses it into
            // ProcStatements, rejecting the forms that have no procedural
            // meaning. Mirrored by `to_proc_statement`.
            TokenKind::For => {
                let header = self.c.span();
                let statement = self.for_block()?;
                to_proc_statement(statement, header)
            }
            // `assignment : IDENT ASSIGN expr` — two tokens of lookahead
            // separate it from an equation.
            TokenKind::Ident(_) if matches!(self.c.peek_at(1), TokenKind::Assign) => {
                let var_name = self.c.expect_ident()?.to_ascii_lowercase();
                self.c.expect(&TokenKind::Assign)?;
                let value = self.expr()?;
                Ok(ProcStatement::Assign { var_name, value })
            }
            _ => {
                // `equation : expr EQ expr` — an intermediate relation.
                let start_pos = self.c.pos();
                let lhs = self.expr()?;
                self.c.expect(&TokenKind::Eq)?;
                let rhs = self.expr()?;
                Ok(ProcStatement::Eq(Equation::new(
                    lhs,
                    rhs,
                    self.text_since(start_pos),
                )))
            }
        }
    }

    /// `ifStatement : IF boolExpr THEN sep procBody (ELSE sep procBody)? END`
    fn if_statement(&mut self) -> Result<ProcStatement> {
        let header = self.c.span();
        self.enter_block(header)?;
        let result = self.if_statement_inner(header);
        self.block_depth -= 1;
        result
    }

    fn if_statement_inner(&mut self, header: Span) -> Result<ProcStatement> {
        self.c.expect(&TokenKind::If)?;
        let condition = parse_bool_expr(&mut self.c)?;
        self.c.expect(&TokenKind::Then)?;
        self.require_sep("after THEN")?;
        let then_branch = self.proc_body(header, "IF", &[TokenKind::Else, TokenKind::End])?;
        let else_branch = if self.c.eat(&TokenKind::Else) {
            self.require_sep("after ELSE")?;
            self.proc_body(header, "IF", &[TokenKind::End])?
        } else {
            Vec::new()
        };
        self.c.expect(&TokenKind::End)?;
        Ok(ProcStatement::IfElse {
            condition,
            then_branch,
            else_branch,
        })
    }

    /// `repeatStatement : REPEAT sep procBody UNTIL boolExpr`
    fn repeat_statement(&mut self) -> Result<ProcStatement> {
        let header = self.c.span();
        self.enter_block(header)?;
        let result = self.repeat_statement_inner(header);
        self.block_depth -= 1;
        result
    }

    fn repeat_statement_inner(&mut self, header: Span) -> Result<ProcStatement> {
        self.c.expect(&TokenKind::Repeat)?;
        self.require_sep("after REPEAT")?;
        let body = self.proc_body(header, "REPEAT", &[TokenKind::Until])?;
        self.c.expect(&TokenKind::Until)?;
        let condition = parse_bool_expr(&mut self.c)?;
        Ok(ProcStatement::RepeatUntil { body, condition })
    }

    /// `whileStatement : WHILE boolExpr DO sep procBody END`
    fn while_statement(&mut self) -> Result<ProcStatement> {
        let header = self.c.span();
        self.enter_block(header)?;
        let result = self.while_statement_inner(header);
        self.block_depth -= 1;
        result
    }

    fn while_statement_inner(&mut self, header: Span) -> Result<ProcStatement> {
        self.c.expect(&TokenKind::While)?;
        let condition = parse_bool_expr(&mut self.c)?;
        self.c.expect(&TokenKind::Do)?;
        self.require_sep("after DO")?;
        let body = self.proc_body(header, "WHILE", &[TokenKind::End])?;
        self.c.expect(&TokenKind::End)?;
        Ok(ProcStatement::While { condition, body })
    }

    // ── shared pieces ───────────────────────────────────────────────────────

    /// Guard a nested block against [`MAX_BLOCK_DEPTH`], charging one level.
    /// The caller must decrement `block_depth` when its inner parse returns.
    fn enter_block(&mut self, header: Span) -> Result<()> {
        if self.block_depth >= MAX_BLOCK_DEPTH {
            return Err(FreesError::parse_at(
                format!("blocks are nested more than {MAX_BLOCK_DEPTH} levels deep"),
                header,
            ));
        }
        self.block_depth += 1;
        Ok(())
    }

    /// Require at least one statement separator, with a message naming where.
    fn require_sep(&mut self, context: &str) -> Result<()> {
        if !self.c.skip_separators() {
            return Err(FreesError::parse_at(
                format!(
                    "expected a line break or `;` {context}, found {}",
                    self.c.peek().describe()
                ),
                self.c.span(),
            ));
        }
        Ok(())
    }

    /// `signedNumber : (PLUS | MINUS)? NUMBER`
    fn signed_number(&mut self) -> Result<f64> {
        let sign = if self.c.eat(&TokenKind::Minus) {
            -1.0
        } else {
            self.c.eat(&TokenKind::Plus);
            1.0
        };
        match self.c.peek().clone() {
            TokenKind::Number { value, .. } => {
                self.c.advance();
                Ok(sign * value)
            }
            other => Err(FreesError::parse_at(
                format!("expected a number, found {}", other.describe()),
                self.c.span(),
            )),
        }
    }

    fn expr(&mut self) -> Result<Expr> {
        (self.expr_fn)(&mut self.c)
    }

    fn new_ignored_sink(&mut self) -> Expr {
        let sink = Expr::Var(format!("{IGNORED_OUTPUT_PREFIX}{}", self.sinks));
        self.sinks += 1;
        sink
    }

    /// The source slice covered by the tokens from `start_pos` up to (but not
    /// including) the cursor. Verbatim — the user's own spacing and inline
    /// comments survive, because diagnostics quote this text.
    fn text_since(&self, start_pos: usize) -> String {
        let end_pos = self.c.pos();
        if end_pos <= start_pos {
            return String::new();
        }
        match (self.tokens.get(start_pos), self.tokens.get(end_pos - 1)) {
            (Some(first), Some(last)) => self
                .c
                .source()
                .get(first.span.start as usize..last.span.end as usize)
                .unwrap_or("")
                .to_string(),
            _ => String::new(),
        }
    }

    /// The span covered by the tokens from `start_pos` up to the cursor.
    fn span_since(&self, start_pos: usize) -> Span {
        let end_pos = self.c.pos();
        match (
            self.tokens.get(start_pos),
            self.tokens.get(end_pos.saturating_sub(1)),
        ) {
            (Some(first), Some(last)) => first.span.merge(last.span),
            _ => self.c.span(),
        }
    }

    // ── lookahead predicates ────────────────────────────────────────────────

    /// `componentInst : IDENT IDENT LPAREN` — two identifiers in a row. Nothing
    /// in the expression grammar can produce that shape, so the three-token
    /// test is exact. Consulted at `topLevel` and inside a `componentItem`; a
    /// `statementList` body does not admit the rule at all.
    fn at_component_inst(&self) -> bool {
        matches!(self.c.peek(), TokenKind::Ident(_))
            && matches!(self.c.peek_at(1), TokenKind::Ident(_))
            && matches!(self.c.peek_at(2), TokenKind::LParen)
    }

    /// True when the `[` under the cursor opens a `multiAssign` rather than a
    /// matrix literal. See the module docs.
    fn at_multi_assign(&self) -> bool {
        if !matches!(self.c.peek(), TokenKind::LBracket) {
            return false;
        }
        let mut i = 1;
        // callOutputs : callOutput (COMMA callOutput)*   with callOutput : IDENT | TILDE
        loop {
            while matches!(self.c.peek_at(i), TokenKind::Newline) {
                i += 1;
            }
            match self.c.peek_at(i) {
                TokenKind::Ident(_) | TokenKind::Tilde => i += 1,
                _ => return false,
            }
            while matches!(self.c.peek_at(i), TokenKind::Newline) {
                i += 1;
            }
            match self.c.peek_at(i) {
                TokenKind::Comma => i += 1,
                TokenKind::RBracket => {
                    i += 1;
                    break;
                }
                _ => return false,
            }
        }
        if !matches!(self.c.peek_at(i), TokenKind::Eq) {
            return false;
        }
        i += 1;
        if !matches!(self.c.peek_at(i), TokenKind::Ident(_)) {
            return false;
        }
        i += 1;
        if !matches!(self.c.peek_at(i), TokenKind::LParen) {
            return false;
        }
        // Skip the balanced argument list; the call must be the whole statement.
        let mut depth: i32 = 0;
        loop {
            match self.c.peek_at(i) {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        // What may legally follow a `statement`. `sep` and end-of-input are the
        // obvious ones; `END` is the third, because `forBlock` (and
        // `moduleDef`) spell their body `statementList sep? END` — the `sep` is
        // optional, so `[a, b] = f(x) END` on one line is a well-formed
        // `multiAssign` for ANTLR. Leaving `END` out here silently demoted it to
        // a matrix-literal equation `[[a, b]] = f(x)`, which binds nothing.
        matches!(
            self.c.peek_at(i),
            TokenKind::Semi | TokenKind::Newline | TokenKind::Eof | TokenKind::End
        )
    }

    /// True when the identifier under the cursor starts a `rangeAssign`:
    /// `IDENT EQ (PLUS|MINUS)? NUMBER COLON …`.
    fn at_range_assign(&self) -> bool {
        if !matches!(self.c.peek(), TokenKind::Ident(_)) {
            return false;
        }
        if !matches!(self.c.peek_at(1), TokenKind::Eq) {
            return false;
        }
        let mut i = 2;
        if matches!(self.c.peek_at(i), TokenKind::Plus | TokenKind::Minus) {
            i += 1;
        }
        if !matches!(self.c.peek_at(i), TokenKind::Number { .. }) {
            return false;
        }
        matches!(self.c.peek_at(i + 1), TokenKind::Colon)
    }
}

// ── free helpers ────────────────────────────────────────────────────────────

/// The block constructs `topLevel` admits that this pass does not implement,
/// keyed by their leading token. `FUNCTION` / `PROCEDURE` / `MODULE` / `TABLE`
/// parse into [`Document::defs`] and are no longer here.
fn unsupported_construct(kind: &TokenKind) -> Option<&'static str> {
    Some(match kind {
        TokenKind::Parametric => "PARAMETRIC",
        TokenKind::StateTable => "STATE TABLE",
        TokenKind::Plot => "PLOT",
        TokenKind::Dynamic => "DYNAMIC",
        TokenKind::Linearize => "LINEARIZE",
        _ => return None,
    })
}

/// A parsed definition on its way into [`Definitions`].
enum ParsedDef {
    Function(FunctionDef),
    Procedure(ProcedureDef),
    Module(ModuleDef),
    Table(FunctionTableDef),
}

/// Insert a definition, evicting any earlier definition of the same name from
/// **all four** kinds first — the Java `LinkedHashMap<String, ProcDef>` keyed
/// by lowercase name (`AstBuilder.buildProgram`) makes a later definition
/// replace an earlier one across kinds. See the module docs.
fn record_def(defs: &mut Definitions, def: ParsedDef) {
    let name = match &def {
        ParsedDef::Function(d) => d.name.clone(),
        ParsedDef::Procedure(d) => d.name.clone(),
        ParsedDef::Module(d) => d.name.clone(),
        ParsedDef::Table(d) => d.name.clone(),
    };
    defs.functions.retain(|d| d.name != name);
    defs.procedures.retain(|d| d.name != name);
    defs.modules.retain(|d| d.name != name);
    defs.tables.retain(|d| d.name != name);
    match def {
        ParsedDef::Function(d) => defs.functions.push(d),
        ParsedDef::Procedure(d) => defs.procedures.push(d),
        ParsedDef::Module(d) => defs.modules.push(d),
        ParsedDef::Table(d) => defs.tables.push(d),
    }
}

/// Convert a top-level [`Statement`] parsed inside a `FOR` body within a
/// procedural body into the equivalent [`ProcStatement`]. Port of
/// `AstBuilder.toProcStatement`: equations and nested `FOR` loops convert
/// recursively; constructs with no procedural meaning are rejected with the
/// Java messages rather than silently dropped.
fn to_proc_statement(statement: Statement, span: Span) -> Result<ProcStatement> {
    match statement {
        Statement::Eq(eq) => Ok(ProcStatement::Eq(eq)),
        Statement::For {
            var_name,
            start,
            end,
            body,
        } => {
            let mut converted = Vec::with_capacity(body.len());
            for inner in body {
                converted.push(to_proc_statement(inner, span)?);
            }
            Ok(ProcStatement::For {
                var_name,
                start,
                end,
                body: converted,
            })
        }
        Statement::CallProc { name, .. } => Err(FreesError::parse_at(
            format!(
                "CALL is not supported inside a FOR loop within a PROCEDURE or \
                 FUNCTION (offending call: '{name}')."
            ),
            span,
        )),
        Statement::Symbolic(_) => Err(FreesError::parse_at(
            "SYMBOLIC declarations are not allowed inside a PROCEDURE or FUNCTION.",
            span,
        )),
    }
}

/// The SI display name of an optional raw unit annotation, or `None` when the
/// annotation is absent or does not parse. Port of `AstBuilder.siUnitOf`,
/// which maps an `UnknownUnitException` to `null` — a bad unit on a
/// declaration never fails the parse.
fn si_unit_of(raw: Option<String>) -> Option<String> {
    let raw = raw?;
    UnitRegistry::parse_with_offset(&raw)
        .ok()
        .map(|quantity| UnitRegistry::si_name(&quantity.dims))
}

/// `None` when no entry carries a unit — the [`FunctionDef::param_units`] /
/// [`FunctionTableDef::arg_units`] contract spells an unannotated declaration
/// as `None` rather than a vector of `None`s.
fn collapse_units(units: Vec<Option<String>>) -> Option<Vec<Option<String>>> {
    if units.iter().all(Option::is_none) {
        None
    } else {
        Some(units)
    }
}

/// Element count of `start:step:stop`, with the same rejections as
/// `AstBuilder.linearRange`.
fn linear_range_count(var: &str, start: f64, step: f64, stop: f64, span: Span) -> Result<i64> {
    if step == 0.0 {
        return Err(FreesError::parse_at(
            format!("Range step is zero in {var} = ..."),
            span,
        ));
    }
    if (stop - start) * step < 0.0 {
        return Err(FreesError::parse_at(
            format!("Range step points the wrong way in {var} = {start}:{step}:{stop}."),
            span,
        ));
    }
    // `(stop - start) / step` is `inf` when the step underflows relative to the
    // span (`0:1e-320:1`) or the bounds overflow (`0:1:1e300`), and `NaN` when
    // both ends are infinite. Java casts that straight to `long`, which
    // saturates at `Long.MAX_VALUE`, and the `+ 1` then wraps to
    // `Long.MIN_VALUE` — sailing past the ceiling check with a *negative*
    // element count. Rust would panic on the same overflow in a debug build and
    // wrap in a release one. Neither is acceptable, so the count is screened
    // while it is still a float and an unrepresentable range is refused with
    // the ceiling message it has earned. See *Deviations*.
    let raw = libm::floor((stop - start) / step + 1e-9);
    if !raw.is_finite() || raw > MAX_RANGE_ELEMENTS as f64 {
        return Err(FreesError::parse_at(
            // Deliberately without the numbers: a step of `1e-320` renders as a
            // 320-digit decimal in `Display`, which buries the message.
            format!(
                "Range {var} = ... would generate more than {MAX_RANGE_ELEMENTS} \
                 elements. Use a larger step."
            ),
            span,
        ));
    }
    let count = raw as i64 + 1;
    if count > MAX_RANGE_ELEMENTS {
        return Err(FreesError::parse_at(
            format!(
                "Range {var} = ... would generate {count} elements \
                 (max {MAX_RANGE_ELEMENTS}). Use a larger step."
            ),
            span,
        ));
    }
    Ok(count)
}

/// Point count of a `| Log` range, with the same rejections as
/// `AstBuilder.logRange`.
fn log_range_count(
    var: &str,
    start: f64,
    count_raw: f64,
    stop: f64,
    three_form: bool,
    span: Span,
) -> Result<i64> {
    if !three_form {
        return Err(FreesError::parse_at(
            format!("A logarithmic range needs start:count:stop (three numbers) in {var} = ..."),
            span,
        ));
    }
    if start <= 0.0 || stop <= 0.0 {
        return Err(FreesError::parse_at(
            format!("A logarithmic range needs positive bounds in {var} = ..."),
            span,
        ));
    }
    let count = libm::round(count_raw) as i64;
    if count < 2 {
        return Err(FreesError::parse_at(
            format!("A logarithmic range needs a point count of at least 2 in {var} = ..."),
            span,
        ));
    }
    if count > MAX_RANGE_ELEMENTS {
        return Err(FreesError::parse_at(
            format!(
                "Range {var} = ... would generate {count} elements (max {MAX_RANGE_ELEMENTS})."
            ),
            span,
        ));
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinOp;

    // ── test double for the expression sub-grammar ──────────────────────────
    //
    // `crate::parser::expr::parse_expr` is owned by another pass and is still a
    // stub, so the statement grammar is driven here by the minimal expression
    // parser below — just enough of `addExpr` / `mulExpr` / `unaryExpr` / `atom`
    // for the statement forms to be exercised. The *lexer* is the real one:
    // `parse` calls `crate::lexer::tokenize`, exactly as `parse_document` does.
    // Production never touches `stub_expr`.

    fn stub_expr(c: &mut Cursor<'_>) -> Result<Expr> {
        stub_add(c)
    }

    fn stub_add(c: &mut Cursor<'_>) -> Result<Expr> {
        let mut left = stub_mul(c)?;
        loop {
            let op = match c.peek() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            c.advance();
            let right = stub_mul(c)?;
            left = Expr::bin(op, left, right);
        }
        Ok(left)
    }

    fn stub_mul(c: &mut Cursor<'_>) -> Result<Expr> {
        let mut left = stub_unary(c)?;
        loop {
            let op = match c.peek() {
                TokenKind::Times => BinOp::Mul,
                TokenKind::Div => BinOp::Div,
                TokenKind::Backslash => BinOp::LeftDiv,
                TokenKind::DotStar => BinOp::ElemMul,
                TokenKind::DotSlash => BinOp::ElemDiv,
                TokenKind::DotBackslash => BinOp::ElemLeftDiv,
                _ => break,
            };
            c.advance();
            let right = stub_unary(c)?;
            left = Expr::bin(op, left, right);
        }
        Ok(left)
    }

    fn stub_unary(c: &mut Cursor<'_>) -> Result<Expr> {
        if c.eat(&TokenKind::Minus) {
            return Ok(Expr::Neg(Box::new(stub_unary(c)?)));
        }
        if c.eat(&TokenKind::Plus) {
            return stub_unary(c);
        }
        let base = stub_atom(c)?;
        if c.eat(&TokenKind::Caret) {
            return Ok(Expr::bin(BinOp::Pow, base, stub_unary(c)?));
        }
        if c.eat(&TokenKind::DotCaret) {
            return Ok(Expr::bin(BinOp::ElemPow, base, stub_unary(c)?));
        }
        Ok(base)
    }

    fn stub_unit(c: &mut Cursor<'_>) -> Option<String> {
        if !matches!(c.peek(), TokenKind::LBracket) {
            return None;
        }
        let open = c.span();
        c.advance();
        while !matches!(c.peek(), TokenKind::RBracket | TokenKind::Eof) {
            c.advance();
        }
        let close = c.span();
        c.eat(&TokenKind::RBracket);
        Some(
            c.source()
                .get(open.end as usize..close.start as usize)
                .unwrap_or("")
                .trim()
                .to_string(),
        )
    }

    fn stub_atom(c: &mut Cursor<'_>) -> Result<Expr> {
        match c.peek().clone() {
            TokenKind::Number { value, .. } => {
                c.advance();
                let unit = stub_unit(c);
                Ok(Expr::Num {
                    value,
                    unit,
                    is_imaginary: false,
                })
            }
            TokenKind::ImagNumber { value, .. } => {
                c.advance();
                let unit = stub_unit(c);
                Ok(Expr::Num {
                    value,
                    unit,
                    is_imaginary: true,
                })
            }
            TokenKind::StringLiteral(s) => {
                c.advance();
                Ok(Expr::Str(s))
            }
            TokenKind::Ident(name) => {
                c.advance();
                if c.eat(&TokenKind::LParen) {
                    let mut args = Vec::new();
                    c.skip_newlines();
                    if !matches!(c.peek(), TokenKind::RParen) {
                        loop {
                            args.push(stub_add(c)?);
                            c.skip_newlines();
                            if c.eat(&TokenKind::Comma) {
                                c.skip_newlines();
                                continue;
                            }
                            break;
                        }
                    }
                    c.expect(&TokenKind::RParen)?;
                    Ok(Expr::call(name, args))
                } else if c.eat(&TokenKind::LBracket) {
                    let mut indices = Vec::new();
                    loop {
                        let low = stub_add(c)?;
                        let index = if c.eat(&TokenKind::Colon) {
                            Expr::Range {
                                start: Box::new(low),
                                end: Box::new(stub_add(c)?),
                            }
                        } else {
                            low
                        };
                        indices.push(index);
                        if c.eat(&TokenKind::Comma) {
                            continue;
                        }
                        break;
                    }
                    c.expect(&TokenKind::RBracket)?;
                    Ok(Expr::ArrayAccess {
                        name: name.to_ascii_lowercase(),
                        indices,
                    })
                } else {
                    Ok(Expr::var(name))
                }
            }
            TokenKind::LBracket => {
                c.advance();
                let mut elements = Vec::new();
                c.skip_newlines();
                if !matches!(c.peek(), TokenKind::RBracket) {
                    loop {
                        elements.push(stub_add(c)?);
                        c.skip_newlines();
                        if c.eat(&TokenKind::Comma) || c.eat(&TokenKind::Semi) {
                            c.skip_newlines();
                            continue;
                        }
                        break;
                    }
                }
                c.expect(&TokenKind::RBracket)?;
                Ok(Expr::ArrayLiteral(elements))
            }
            TokenKind::LParen => {
                c.advance();
                let inner = stub_add(c)?;
                c.expect(&TokenKind::RParen)?;
                Ok(inner)
            }
            other => Err(FreesError::parse_at(
                format!("expected an expression, found {}", other.describe()),
                c.span(),
            )),
        }
    }

    // ── harness ─────────────────────────────────────────────────────────────

    fn parse(src: &str) -> Result<Document> {
        let tokens = crate::lexer::tokenize(src)?;
        parse_token_stream(src, &tokens, stub_expr)
    }

    fn ok(src: &str) -> Document {
        parse(src).unwrap_or_else(|e| panic!("expected `{src}` to parse, got {e}"))
    }

    fn err(src: &str) -> String {
        match parse(src) {
            Ok(doc) => panic!("expected `{src}` to fail, got {doc:?}"),
            Err(e) => e.to_string(),
        }
    }

    fn eq_of(s: &Statement) -> Equation {
        match s {
            Statement::Eq(e) => e.clone(),
            other => panic!("expected an equation, got {other:?}"),
        }
    }

    // ── documents, separators, trivia ───────────────────────────────────────

    #[test]
    fn empty_document_is_not_an_error() {
        let doc = ok("");
        assert!(doc.statements.is_empty());
        assert!(doc.guesses.is_empty());
        assert!(doc.diagnostics.is_empty());
    }

    #[test]
    fn whitespace_and_comment_only_documents_are_empty() {
        for src in [
            "\n\n\n",
            "   \t  ",
            "{ a brace comment }\n",
            "// a line comment\n\n",
            "\"a quote comment\"\n",
            "\n  { one }  // two\n  \"three\"  \n\n",
            ";;;\n;\n",
        ] {
            let doc = ok(src);
            assert!(
                doc.statements.is_empty() && doc.guesses.is_empty(),
                "`{src}` should produce nothing, got {doc:?}"
            );
        }
    }

    #[test]
    fn statements_are_separated_by_newlines_and_semicolons() {
        let doc = ok("a = 1\nb = 2; c = 3\n\n;\nd = 4");
        assert_eq!(doc.statements.len(), 4);
        let names: Vec<_> = doc
            .statements
            .iter()
            .map(|s| match &eq_of(s).lhs {
                Expr::Var(v) => v.clone(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(names, ["a", "b", "c", "d"]);
    }

    #[test]
    fn leading_and_trailing_separators_are_tolerated() {
        let doc = ok("\n\n;  x = 1  ;\n\n");
        assert_eq!(doc.statements.len(), 1);
    }

    #[test]
    fn two_statements_on_one_line_without_a_separator_is_an_error() {
        let message = err("a = 1 b = 2");
        assert!(
            message.contains("expected end of statement"),
            "got: {message}"
        );
    }

    // ── equations ───────────────────────────────────────────────────────────

    #[test]
    fn an_equation_becomes_a_residual_pair() {
        let doc = ok("T_out = T_in + dT");
        let eq = eq_of(&doc.statements[0]);
        assert_eq!(eq.lhs, Expr::var("t_out"));
        assert_eq!(
            eq.rhs,
            Expr::bin(BinOp::Add, Expr::var("t_in"), Expr::var("dt"))
        );
    }

    #[test]
    fn source_text_is_captured_verbatim_not_re_rendered() {
        // The Java builder stores `ctx.getText()`, which strips whitespace.
        // Here the user's own spacing and inline comment survive intact.
        let src = "x   =  1 +    2 { why }\ny = 3";
        let doc = ok(src);
        assert_eq!(eq_of(&doc.statements[0]).source_text, "x   =  1 +    2");
        assert_eq!(eq_of(&doc.statements[1]).source_text, "y = 3");
    }

    #[test]
    fn source_text_of_the_second_statement_starts_at_its_own_first_token() {
        let doc = ok("a = 1; bee = a*2");
        assert_eq!(eq_of(&doc.statements[1]).source_text, "bee = a*2");
    }

    #[test]
    fn an_equation_needs_an_equals_sign() {
        let message = err("x + 1\n");
        assert!(message.contains("expected `=`"), "got: {message}");
    }

    #[test]
    fn a_stray_end_is_reported_as_such() {
        let message = err("END");
        assert!(message.contains("unexpected `END`"), "got: {message}");
    }

    // ── FOR blocks ──────────────────────────────────────────────────────────

    #[test]
    fn for_block_collects_its_body() {
        let doc = ok("FOR i = 1 TO N\n  x[i] = i * 2\n  y[i] = x[i]\nEND");
        assert_eq!(doc.statements.len(), 1);
        match &doc.statements[0] {
            Statement::For {
                var_name,
                start,
                end,
                body,
            } => {
                assert_eq!(var_name, "i");
                assert_eq!(*start, Expr::num(1.0));
                assert_eq!(*end, Expr::var("n"));
                assert_eq!(body.len(), 2);
            }
            other => panic!("expected a FOR block, got {other:?}"),
        }
    }

    #[test]
    fn for_keyword_and_loop_variable_are_case_insensitive() {
        let doc = ok("for I = 1 to 3\n  a[I] = 0\nend");
        match &doc.statements[0] {
            Statement::For { var_name, body, .. } => {
                assert_eq!(var_name, "i");
                assert_eq!(body.len(), 1);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn for_blocks_nest() {
        let doc = ok("FOR i = 1 TO 2\n\
             \x20 FOR j = 1 TO 3\n\
             \x20   a[i] = j\n\
             \x20 END\n\
             \x20 b[i] = i\n\
             END");
        let outer = match &doc.statements[0] {
            Statement::For { body, .. } => body,
            other => panic!("{other:?}"),
        };
        assert_eq!(outer.len(), 2);
        match &outer[0] {
            Statement::For { var_name, body, .. } => {
                assert_eq!(var_name, "j");
                assert_eq!(body.len(), 1);
            }
            other => panic!("expected a nested FOR, got {other:?}"),
        }
        // `Document::equations()` flattens both levels.
        assert_eq!(doc.equations().len(), 2);
    }

    #[test]
    fn an_empty_for_body_is_allowed() {
        let doc = ok("FOR i = 1 TO 3\nEND");
        match &doc.statements[0] {
            Statement::For { body, .. } => assert!(body.is_empty()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn for_body_statements_may_be_semicolon_separated() {
        let doc = ok("FOR i = 1 TO 3; a[i] = 1; b[i] = 2; END");
        match &doc.statements[0] {
            Statement::For { body, .. } => assert_eq!(body.len(), 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_unterminated_for_block_names_the_missing_end() {
        let message = err("FOR i = 1 TO 3\n  a[i] = 1\n");
        assert!(
            message.contains("unterminated FOR block") && message.contains("END"),
            "got: {message}"
        );
    }

    #[test]
    fn the_for_header_must_end_the_line() {
        let message = err("FOR i = 1 TO 3 a[i] = 1 END");
        assert!(message.contains("after the FOR header"), "got: {message}");
    }

    #[test]
    fn for_bounds_may_be_arbitrary_expressions() {
        let doc = ok("FOR k = n_start + 1 TO 2 * n\n  q[k] = 0\nEND");
        match &doc.statements[0] {
            Statement::For { start, end, .. } => {
                assert_eq!(
                    *start,
                    Expr::bin(BinOp::Add, Expr::var("n_start"), Expr::num(1.0))
                );
                assert_eq!(*end, Expr::bin(BinOp::Mul, Expr::num(2.0), Expr::var("n")));
            }
            other => panic!("{other:?}"),
        }
    }

    // ── CALL ────────────────────────────────────────────────────────────────

    #[test]
    fn call_statement_splits_inputs_from_outputs() {
        let doc = ok("CALL Split(x, y : q, w)");
        match &doc.statements[0] {
            Statement::CallProc {
                name,
                inputs,
                outputs,
                source_text,
            } => {
                assert_eq!(name, "split");
                assert_eq!(inputs, &[Expr::var("x"), Expr::var("y")]);
                assert_eq!(outputs, &[Expr::var("q"), Expr::var("w")]);
                assert_eq!(source_text, "CALL Split(x, y : q, w)");
            }
            other => panic!("expected a CALL, got {other:?}"),
        }
    }

    #[test]
    fn call_argument_lists_may_be_empty_on_either_side() {
        match &ok("call f( : a)").statements[0] {
            Statement::CallProc {
                inputs, outputs, ..
            } => {
                assert!(inputs.is_empty());
                assert_eq!(outputs.len(), 1);
            }
            other => panic!("{other:?}"),
        }
        match &ok("call f(a : )").statements[0] {
            Statement::CallProc {
                inputs, outputs, ..
            } => {
                assert_eq!(inputs.len(), 1);
                assert!(outputs.is_empty());
            }
            other => panic!("{other:?}"),
        }
        match &ok("call f( : )").statements[0] {
            Statement::CallProc {
                inputs, outputs, ..
            } => {
                assert!(inputs.is_empty() && outputs.is_empty());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn call_arguments_are_full_expressions() {
        match &ok("CALL pump(m * 2, h(1) : W_dot)").statements[0] {
            Statement::CallProc { inputs, .. } => {
                assert_eq!(
                    inputs[0],
                    Expr::bin(BinOp::Mul, Expr::var("m"), Expr::num(2.0))
                );
                assert_eq!(inputs[1], Expr::call("h", vec![Expr::num(1.0)]));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_call_without_the_colon_is_rejected() {
        let message = err("CALL f(a, b)");
        assert!(message.contains("expected `:`"), "got: {message}");
    }

    // ── SYMBOLIC ────────────────────────────────────────────────────────────

    #[test]
    fn symbolic_declares_lowercased_names() {
        let doc = ok("SYMBOLIC s, T, Omega");
        assert_eq!(
            doc.statements[0],
            Statement::Symbolic(vec!["s".into(), "t".into(), "omega".into()])
        );
    }

    #[test]
    fn symbolic_accepts_a_single_name() {
        assert_eq!(
            ok("symbolic s").statements[0],
            Statement::Symbolic(vec!["s".into()])
        );
    }

    #[test]
    fn symbolic_needs_at_least_one_name() {
        let message = err("SYMBOLIC");
        assert!(message.contains("expected an identifier"), "got: {message}");
    }

    // ── multiAssign ─────────────────────────────────────────────────────────

    #[test]
    fn multi_assign_desugars_to_a_call_proc() {
        let doc = ok("[q, w] = Split(x)");
        match &doc.statements[0] {
            Statement::CallProc {
                name,
                inputs,
                outputs,
                source_text,
            } => {
                assert_eq!(name, "split");
                assert_eq!(inputs, &[Expr::var("x")]);
                assert_eq!(outputs, &[Expr::var("q"), Expr::var("w")]);
                assert_eq!(source_text, "[q, w] = Split(x)");
            }
            other => panic!("expected a CALL, got {other:?}"),
        }
    }

    #[test]
    fn a_discarded_output_slot_binds_a_fresh_unforgeable_sink() {
        let doc = ok("[~, ~, V] = svd(A)\n[~, B] = qr(A)");
        let first = match &doc.statements[0] {
            Statement::CallProc { outputs, .. } => outputs.clone(),
            other => panic!("{other:?}"),
        };
        let second = match &doc.statements[1] {
            Statement::CallProc { outputs, .. } => outputs.clone(),
            other => panic!("{other:?}"),
        };
        let sink_names: Vec<String> = first
            .iter()
            .chain(second.iter())
            .filter_map(|e| match e {
                Expr::Var(n) if is_ignored_sink(n) => Some(n.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(sink_names.len(), 3, "three `~` slots");
        // Unique within the document, and unforgeable (a user IDENT starts
        // with a letter, so it can never collide).
        let mut sorted = sink_names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 3);
        assert!(sink_names
            .iter()
            .all(|n| n.starts_with(IGNORED_OUTPUT_PREFIX)));
        assert_eq!(first[2], Expr::var("v"));
        assert_eq!(second[1], Expr::var("b"));
    }

    #[test]
    fn sink_numbering_is_deterministic_across_parses() {
        let a = ok("[~, x] = f(1)");
        let b = ok("[~, x] = f(1)");
        assert_eq!(a.statements, b.statements);
    }

    #[test]
    fn a_single_output_destructure_still_wins_over_a_matrix_equation() {
        // The grammar lists multiAssign before equation for exactly this case.
        match &ok("[a] = f(x)").statements[0] {
            Statement::CallProc { outputs, .. } => {
                assert_eq!(outputs, &[Expr::var("a")]);
            }
            other => panic!("expected a CALL, got {other:?}"),
        }
    }

    #[test]
    fn a_matrix_literal_equation_is_not_a_multi_assign() {
        // rhs is not `IDENT (`, so this stays an ordinary equation.
        let eq = eq_of(&ok("[a, b] = [1, 2]").statements[0]);
        assert_eq!(
            eq.lhs,
            Expr::ArrayLiteral(vec![Expr::var("a"), Expr::var("b")])
        );
        assert_eq!(
            eq.rhs,
            Expr::ArrayLiteral(vec![Expr::num(1.0), Expr::num(2.0)])
        );
    }

    #[test]
    fn a_call_that_is_not_the_whole_statement_is_an_equation() {
        // `[a, b] = f(x) + 1` cannot be a multiAssign — the rule ends at `)`.
        let eq = eq_of(&ok("[a, b] = f(x) + 1").statements[0]);
        assert_eq!(
            eq.lhs,
            Expr::ArrayLiteral(vec![Expr::var("a"), Expr::var("b")])
        );
        assert_eq!(
            eq.rhs,
            Expr::bin(
                BinOp::Add,
                Expr::call("f", vec![Expr::var("x")]),
                Expr::num(1.0)
            )
        );
    }

    #[test]
    fn a_matrix_lhs_with_a_scalar_rhs_is_an_equation() {
        let eq = eq_of(&ok("[a, b] = c").statements[0]);
        assert_eq!(eq.rhs, Expr::var("c"));
    }

    #[test]
    fn multi_assign_survives_a_semicolon_terminator() {
        match &ok("[a, b] = tf2ss(num, den); x = 1").statements[0] {
            Statement::CallProc { name, inputs, .. } => {
                assert_eq!(name, "tf2ss");
                assert_eq!(inputs.len(), 2);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn nested_parentheses_in_the_argument_list_do_not_confuse_the_lookahead() {
        match &ok("[a, b] = g(f(x, (y + 1)), z)").statements[0] {
            Statement::CallProc { name, inputs, .. } => {
                assert_eq!(name, "g");
                assert_eq!(inputs.len(), 2);
            }
            other => panic!("expected a CALL, got {other:?}"),
        }
    }

    // ── rangeAssign ─────────────────────────────────────────────────────────

    fn range_args(doc: &Document) -> Vec<Expr> {
        match &eq_of(&doc.statements[0]).rhs {
            Expr::Call { function, args } => {
                assert_eq!(function, RANGE_INTRINSIC);
                args.clone()
            }
            other => panic!("expected a range call, got {other:?}"),
        }
    }

    #[test]
    fn a_three_number_range_lowers_to_the_range_intrinsic() {
        let doc = ok("speed = 0:10:100");
        assert_eq!(eq_of(&doc.statements[0]).lhs, Expr::var("speed"));
        assert_eq!(
            range_args(&doc),
            vec![
                Expr::num(0.0),
                Expr::num(10.0),
                Expr::num(100.0),
                Expr::Str("linear".into())
            ]
        );
        assert_eq!(eq_of(&doc.statements[0]).source_text, "speed = 0:10:100");
    }

    #[test]
    fn a_two_number_range_defaults_to_step_one() {
        let doc = ok("k = 1:5");
        assert_eq!(
            range_args(&doc),
            vec![
                Expr::num(1.0),
                Expr::num(1.0),
                Expr::num(5.0),
                Expr::Str("linear".into())
            ]
        );
    }

    #[test]
    fn a_range_may_count_down_and_carry_signs() {
        let doc = ok("t = 10:-2:-10");
        assert_eq!(
            range_args(&doc),
            vec![
                Expr::num(10.0),
                Expr::num(-2.0),
                Expr::num(-10.0),
                Expr::Str("linear".into())
            ]
        );
    }

    #[test]
    fn the_spacing_flag_is_lowercased_and_recorded() {
        let doc = ok("freq = 1:5:1000 | Log");
        assert_eq!(
            range_args(&doc),
            vec![
                Expr::num(1.0),
                Expr::num(5.0),
                Expr::num(1000.0),
                Expr::Str("log".into())
            ]
        );
        let doc = ok("freq = 1:5:1000 | LINEAR");
        assert_eq!(range_args(&doc)[3], Expr::Str("linear".into()));
    }

    #[test]
    fn a_plain_assignment_is_an_equation_not_a_range() {
        let eq = eq_of(&ok("n = 5").statements[0]);
        assert_eq!(eq.lhs, Expr::var("n"));
        assert_eq!(eq.rhs, Expr::num(5.0));
    }

    #[test]
    fn an_array_slice_equation_is_not_a_range_assign() {
        // `x[1:3] = ...` has `[` after the name, so the rangeAssign lookahead
        // (IDENT EQ NUMBER COLON) never fires.
        let eq = eq_of(&ok("x[1:3] = y").statements[0]);
        match &eq.lhs {
            Expr::ArrayAccess { name, indices } => {
                assert_eq!(name, "x");
                assert_eq!(
                    indices[0],
                    Expr::Range {
                        start: Box::new(Expr::num(1.0)),
                        end: Box::new(Expr::num(3.0))
                    }
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_range_with_a_zero_step_is_rejected() {
        let message = err("x = 0:0:10");
        assert!(message.contains("Range step is zero"), "got: {message}");
    }

    #[test]
    fn a_range_whose_step_points_the_wrong_way_is_rejected() {
        let message = err("x = 0:-1:10");
        assert!(message.contains("points the wrong way"), "got: {message}");
    }

    #[test]
    fn an_absurdly_fine_range_is_rejected_at_parse_time() {
        let message = err("x = 0:0.0000001:100");
        assert!(
            message.contains("would generate") && message.contains("100000"),
            "got: {message}"
        );
    }

    #[test]
    fn a_log_range_needs_three_numbers() {
        let message = err("x = 1:1000 | Log");
        assert!(message.contains("start:count:stop"), "got: {message}");
    }

    #[test]
    fn a_log_range_needs_positive_bounds_and_two_points() {
        assert!(err("x = 0:5:100 | Log").contains("positive bounds"));
        assert!(err("x = -1:5:100 | log").contains("positive bounds"));
        assert!(err("x = 1:1:100 | Log").contains("at least 2"));
    }

    #[test]
    fn an_unknown_range_spacing_names_the_supported_ones() {
        let message = err("x = 1:2:10 | Geometric");
        assert!(
            message.contains("Unknown range spacing 'Geometric'")
                && message.contains("Linear, Log"),
            "got: {message}"
        );
    }

    // ── GUESS ───────────────────────────────────────────────────────────────

    #[test]
    fn guess_supports_all_three_spellings() {
        let doc = ok("GUESS x = 2\nGUESS y = 2 [0, 10]\nGUESS z [0, 10]");
        assert!(doc.statements.is_empty(), "GUESS is not a statement");
        assert_eq!(
            doc.guesses,
            vec![
                GuessDirective {
                    name: "x".into(),
                    guess: Some(2.0),
                    lower: None,
                    upper: None
                },
                GuessDirective {
                    name: "y".into(),
                    guess: Some(2.0),
                    lower: Some(0.0),
                    upper: Some(10.0)
                },
                GuessDirective {
                    name: "z".into(),
                    guess: None,
                    lower: Some(0.0),
                    upper: Some(10.0)
                },
            ]
        );
    }

    #[test]
    fn guess_lowercases_the_variable_and_accepts_signed_numbers() {
        let doc = ok("guess T_In = -3.5 [-10, +10]");
        assert_eq!(
            doc.guesses[0],
            GuessDirective {
                name: "t_in".into(),
                guess: Some(-3.5),
                lower: Some(-10.0),
                upper: Some(10.0)
            }
        );
    }

    #[test]
    fn guess_travels_alongside_the_statements() {
        let doc = ok("x = y ^ 2\nGUESS y = 1\ny + x = 4");
        assert_eq!(doc.statements.len(), 2);
        assert_eq!(doc.guesses.len(), 1);
    }

    #[test]
    fn a_guess_that_declares_nothing_is_rejected() {
        let message = err("GUESS x");
        assert!(
            message.contains("declare a guess") && message.contains("GUESS x [0, 10]"),
            "got: {message}"
        );
    }

    #[test]
    fn crossed_guess_bounds_are_rejected() {
        let message = err("GUESS x [10, 0]");
        assert!(
            message.contains("lower bound must be below the upper bound"),
            "got: {message}"
        );
        assert!(err("GUESS x [5, 5]").contains("lower bound must be below"));
    }

    #[test]
    fn a_guess_outside_its_own_bounds_is_rejected() {
        let message = err("GUESS x = 20 [0, 10]");
        assert!(message.contains("lies outside"), "got: {message}");
        assert!(err("GUESS x = -1 [0, 10]").contains("lies outside"));
    }

    #[test]
    fn a_malformed_guess_bound_list_is_rejected() {
        assert!(err("GUESS x [0 10]").contains("expected `,`"));
        assert!(err("GUESS x = ").contains("expected a number"));
    }

    // ── unsupported constructs ──────────────────────────────────────────────

    #[test]
    fn every_unimplemented_block_is_named_in_its_error() {
        let cases = [
            ("PARAMETRIC sweep(a)\n  a = 1:2:3\nEND", "PARAMETRIC"),
            (
                "STATE TABLE circuit(P1)\n  FLUID = Water\nEND",
                "STATE TABLE",
            ),
            ("PLOT 'speed'\n  kind = xy\nEND", "PLOT"),
            ("DYNAMIC d(method = ode45)\n  der = 1\nEND", "DYNAMIC"),
            ("LINEARIZE plant(block = w)\n  INPUT q\nEND", "LINEARIZE"),
        ];
        for (src, construct) in cases {
            let message = err(src);
            assert!(
                message.contains(construct) && message.contains("not supported"),
                "`{construct}` should be refused by name, got: {message}"
            );
        }
    }

    /// `COMPONENT` and `connect` left the refusal list in Phase 6 — the parser
    /// now builds their AST. (Whether the *engine* can honour the result is a
    /// separate gate: see `engine::reject_unexpanded_components`.)
    #[test]
    fn the_component_forms_are_no_longer_refused_by_the_parser() {
        assert!(unsupported_construct(&TokenKind::Component).is_none());
        assert!(unsupported_construct(&TokenKind::Connect).is_none());
        for still_refused in [
            TokenKind::Parametric,
            TokenKind::StateTable,
            TokenKind::Plot,
            TokenKind::Dynamic,
            TokenKind::Linearize,
        ] {
            assert!(
                unsupported_construct(&still_refused).is_some(),
                "{still_refused:?} stays refused until its own phase"
            );
        }
    }

    #[test]
    fn unsupported_errors_are_anchored_to_the_offending_token() {
        let src = "x = 1\nPLOT 'speed'\n  kind = xy\nEND";
        let error = parse(src).unwrap_err();
        let span = error.span().expect("an unsupported error carries a span");
        assert_eq!(span.slice(src), "PLOT");
        assert_eq!(span.line_col(src), (2, 1));
    }

    #[test]
    fn an_unsupported_block_inside_a_for_body_is_still_refused_by_name() {
        let message = err("FOR i = 1 TO 2\n  PLOT 'x'\n  END\nEND");
        assert!(message.contains("PLOT"), "got: {message}");
    }

    #[test]
    fn a_bare_state_identifier_is_not_the_state_table_keyword() {
        // `STATE TABLE` is one token; a lone `state` stays an identifier.
        let eq = eq_of(&ok("state = 1").statements[0]);
        assert_eq!(eq.lhs, Expr::var("state"));
    }

    // ── mixed documents ─────────────────────────────────────────────────────

    #[test]
    fn a_realistic_document_parses_end_to_end() {
        let src = "\
{ pipe network }
GUESS mdot = 0.5 [0, 10]
SYMBOLIC s

mdot = rho * A * v        // continuity
speed = 0:10:100
[q, ~] = split(mdot)
CALL pump(mdot, dP : W_dot)

FOR i = 1 TO 3
  dp[i] = f * i
END
";
        let doc = ok(src);
        assert_eq!(doc.guesses.len(), 1);
        assert_eq!(doc.statements.len(), 6);
        assert!(matches!(doc.statements[0], Statement::Symbolic(_)));
        assert!(matches!(doc.statements[1], Statement::Eq(_)));
        assert!(matches!(doc.statements[2], Statement::Eq(_))); // the range
        assert!(matches!(doc.statements[3], Statement::CallProc { .. }));
        assert!(matches!(doc.statements[4], Statement::CallProc { .. }));
        assert!(matches!(doc.statements[5], Statement::For { .. }));
        // `equations()` sees the plain equation, the lowered range, and the FOR
        // body — but not the CALLs.
        assert_eq!(doc.equations().len(), 3);
        assert_eq!(doc.equations()[0].source_text, "mdot = rho * A * v");
    }

    /// Everything reachable without the expression sub-grammar, driven through
    /// the real public entry point — this is the only place that proves
    /// `parse_document` itself is wired to the real lexer. (Statement forms
    /// containing an expression cannot go through it until
    /// `parser::expr::parse_expr` lands.)
    #[test]
    fn the_public_entry_point_is_wired_up() {
        assert_eq!(parse_document("").unwrap(), Document::default());
        assert_eq!(
            parse_document("\n  { just a comment }  // and another\n\n").unwrap(),
            Document::default()
        );

        let doc = parse_document("GUESS T_hot = 500 [300, 900]\nSYMBOLIC s, z").unwrap();
        assert_eq!(
            doc.guesses,
            vec![GuessDirective {
                name: "t_hot".into(),
                guess: Some(500.0),
                lower: Some(300.0),
                upper: Some(900.0),
            }]
        );
        assert_eq!(
            doc.statements,
            vec![Statement::Symbolic(vec!["s".into(), "z".into()])]
        );

        let error = parse_document("PLOT 'x'\n  kind = xy\nEND").unwrap_err();
        assert!(
            error.to_string().contains("PLOT") && error.to_string().contains("not supported"),
            "got: {error}"
        );

        assert!(parse_document("END")
            .unwrap_err()
            .to_string()
            .contains("unexpected `END`"));
    }

    #[test]
    fn ignored_sink_names_cannot_be_written_by_a_user() {
        assert!(is_ignored_sink("~ignored~0"));
        assert!(!is_ignored_sink("ignored0"));
        assert!(!is_ignored_sink("x"));
    }

    // ── FUNCTION / PROCEDURE / MODULE / TABLE definitions ───────────────────
    //
    // These go through `parse_document` (the real expression parser), since
    // procedural bodies lean on `boolExpr` and the full expression grammar.

    fn ok_real(src: &str) -> Document {
        parse_document(src).unwrap_or_else(|e| panic!("expected `{src}` to parse, got {e}"))
    }

    fn err_real(src: &str) -> String {
        match parse_document(src) {
            Ok(doc) => panic!("expected `{src}` to fail, got {doc:?}"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn a_function_parses_into_defs_with_its_body() {
        let doc = ok_real("FUNCTION Square(x)\n  Square := x * x\nEND\nz = Square(4)");
        assert_eq!(doc.statements.len(), 1, "the equation after the def");
        assert_eq!(doc.defs.functions.len(), 1);
        let f = doc.defs.function("square").expect("registered lowercase");
        assert_eq!(f.params, vec!["x"]);
        assert_eq!(f.output_unit, None);
        assert_eq!(f.param_units, None);
        assert_eq!(
            f.body,
            vec![ProcStatement::Assign {
                var_name: "square".into(),
                value: Expr::bin(BinOp::Mul, Expr::var("x"), Expr::var("x")),
            }]
        );
    }

    #[test]
    fn a_function_body_supports_if_then_else_over_a_bool_condition() {
        let doc = ok_real("FUNCTION AbsVal(x)\n  IF x >= 0 THEN\n    AbsVal := x\n  ELSE\n    AbsVal := -x\n  END\nEND");
        let f = doc.defs.function("absval").unwrap();
        match &f.body[0] {
            ProcStatement::IfElse {
                condition,
                then_branch,
                else_branch,
            } => {
                assert_eq!(
                    *condition,
                    Expr::Compare {
                        op: crate::ast::CmpOp::Ge,
                        left: Box::new(Expr::var("x")),
                        right: Box::new(Expr::num(0.0)),
                    }
                );
                assert_eq!(then_branch.len(), 1);
                assert_eq!(else_branch.len(), 1);
            }
            other => panic!("expected IF, got {other:?}"),
        }
    }

    #[test]
    fn an_if_without_else_has_an_empty_else_branch() {
        let doc = ok_real("FUNCTION f(x)\n  f := 0\n  IF x > 1 THEN\n    f := 1\n  END\nEND");
        match &doc.defs.function("f").unwrap().body[1] {
            ProcStatement::IfElse { else_branch, .. } => assert!(else_branch.is_empty()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn repeat_until_and_while_do_parse() {
        let doc = ok_real(
            "FUNCTION SumN(n)\n  i := 1\n  s := 0\n  REPEAT\n    s := s + i\n    i := i + 1\n  UNTIL i > n\n  SumN := s\nEND",
        );
        match &doc.defs.function("sumn").unwrap().body[2] {
            ProcStatement::RepeatUntil { body, condition } => {
                assert_eq!(body.len(), 2);
                assert!(matches!(condition, Expr::Compare { .. }));
            }
            other => panic!("expected REPEAT, got {other:?}"),
        }

        let doc = ok_real(
            "FUNCTION SumWhile(n)\n  s := 0\n  WHILE s < n DO\n    s := s + 1\n  END\n  SumWhile := s\nEND",
        );
        match &doc.defs.function("sumwhile").unwrap().body[1] {
            ProcStatement::While { body, .. } => assert_eq!(body.len(), 1),
            other => panic!("expected WHILE, got {other:?}"),
        }
    }

    #[test]
    fn a_bare_equation_in_a_body_is_kept_as_an_eq_statement() {
        let doc = ok_real("FUNCTION f(x)\n  y = x + 1\n  f := y\nEND");
        match &doc.defs.function("f").unwrap().body[0] {
            ProcStatement::Eq(eq) => {
                assert_eq!(eq.lhs, Expr::var("y"));
                assert_eq!(eq.source_text, "y = x + 1");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_for_inside_a_body_reuses_the_statement_grammar_and_converts() {
        // `nestedForInsideFunctionAccumulatesCorrectly` (oracle): the FOR body
        // is a statement list (`=`, not `:=`) converted to ProcStatements.
        let doc = ok_real(
            "FUNCTION DoubleSum(n)\n  s := 0\n  FOR i = 1 TO n\n    FOR j = 1 TO n\n      s = s + i * j\n    END\n  END\n  DoubleSum := s\nEND",
        );
        match &doc.defs.function("doublesum").unwrap().body[1] {
            ProcStatement::For { var_name, body, .. } => {
                assert_eq!(var_name, "i");
                match &body[0] {
                    ProcStatement::For { var_name, body, .. } => {
                        assert_eq!(var_name, "j");
                        assert!(matches!(&body[0], ProcStatement::Eq(_)));
                    }
                    other => panic!("expected nested FOR, got {other:?}"),
                }
            }
            other => panic!("expected FOR, got {other:?}"),
        }
    }

    #[test]
    fn call_inside_a_for_within_a_body_is_rejected_by_name() {
        // Oracle: `callInsideProcedureForIsRejectedNotSilentlyDropped`.
        let message = err_real(
            "FUNCTION Bad(n)\n  FOR i = 1 TO n\n    CALL pole(i, i : a, b)\n  END\n  Bad := 1\nEND",
        );
        assert!(
            message.contains("CALL") && message.contains("'pole'"),
            "got: {message}"
        );
    }

    #[test]
    fn symbolic_inside_a_for_within_a_body_is_rejected() {
        let message =
            err_real("FUNCTION Bad(n)\n  FOR i = 1 TO n\n    SYMBOLIC s\n  END\n  Bad := 1\nEND");
        assert!(message.contains("SYMBOLIC"), "got: {message}");
    }

    #[test]
    fn function_units_convert_to_si_display_names() {
        // `FUNCTION f(v [km/h], m) [kW]` — kW grounds to W, km/h to m/s;
        // unknown units map to None exactly like `siUnitOf`.
        let doc = ok_real("FUNCTION f(v [km/h], m) [kW]\n  f := v * m\nEND");
        let f = doc.defs.function("f").unwrap();
        assert_eq!(f.output_unit.as_deref(), Some("W"));
        assert_eq!(
            f.param_units,
            Some(vec![Some("m/s".to_string()), None]),
            "aligned to the parameter list"
        );

        let doc = ok_real("FUNCTION g(x [zorp])\n  g := x\nEND");
        assert_eq!(doc.defs.function("g").unwrap().param_units, None);
    }

    #[test]
    fn a_multi_output_function_desugars_to_a_procedure() {
        // AstBuilder.buildFunctionDef: `FUNCTION [q, r] = DivMod(a, b)` becomes
        // a ProcedureDef (the FunctionDef record has no outputs field).
        let doc =
            ok_real("FUNCTION [q, r] = DivMod(a, b)\n  q := trunc(a / b)\n  r := a - q * b\nEND");
        assert!(doc.defs.functions.is_empty());
        let p = doc.defs.procedure("divmod").expect("lowered to PROCEDURE");
        assert_eq!(p.inputs, vec!["a", "b"]);
        assert_eq!(p.outputs, vec!["q", "r"]);
        assert_eq!(p.body.len(), 2);
    }

    #[test]
    fn a_procedure_parses_with_inputs_and_outputs_split() {
        let doc = ok_real("PROCEDURE Swap(a, b : c, d)\n  c := b\n  d := a\nEND");
        let p = doc.defs.procedure("swap").unwrap();
        assert_eq!(p.inputs, vec!["a", "b"]);
        assert_eq!(p.outputs, vec!["c", "d"]);
        assert_eq!(p.body.len(), 2);
        // Either side of the colon may be empty, and bodies may be empty.
        let doc = ok_real("PROCEDURE p( : y)\nEND");
        let p = doc.defs.procedure("p").unwrap();
        assert!(p.inputs.is_empty() && p.outputs == vec!["y"] && p.body.is_empty());
    }

    #[test]
    fn a_module_body_is_a_statement_list() {
        let doc = ok_real("MODULE Linear(m, b : y)\n  y = m * x_int + b\n  x_int = 3\nEND");
        let m = doc.defs.module("linear").unwrap();
        assert_eq!(m.inputs, vec!["m", "b"]);
        assert_eq!(m.outputs, vec!["y"]);
        assert_eq!(m.body.len(), 2);
        assert!(matches!(&m.body[0], Statement::Eq(_)));
    }

    #[test]
    fn a_1d_table_parses_rows_into_one_curve_sorted_by_x() {
        // Rows deliberately out of order: buildCurves sorts ascending by x.
        let doc = ok_real("TABLE friction(Re)\n  4000 0.04\n  2000 0.049\n  10000 0.031\nEND");
        let t = doc.defs.table("friction").unwrap();
        assert_eq!(t.arg_names, vec!["re"]);
        assert!(!t.x_log && !t.y_log);
        assert_eq!(t.curves.len(), 1);
        assert_eq!(t.curves[0].param, None);
        assert_eq!(t.curves[0].xs, vec![2000.0, 4000.0, 10000.0]);
        assert_eq!(t.curves[0].ys, vec![0.049, 0.04, 0.031]);
        assert_eq!(t.output_unit, None);
        assert_eq!(t.arg_units, None);
    }

    #[test]
    fn table_rows_take_signed_numbers() {
        let doc = ok_real("TABLE f(x)\n  -2 4\n  -1 1\n  +1 1\nEND");
        let t = doc.defs.table("f").unwrap();
        assert_eq!(t.curves[0].xs, vec![-2.0, -1.0, 1.0]);
    }

    #[test]
    fn a_family_table_parses_params_flags_and_units() {
        let doc =
            ok_real("TABLE cp(T [C] : P = 100, 200) [kJ/kg-K] XLOG YLOG\n  1 2 3\n  10 4 5\nEND");
        let t = doc.defs.table("cp").unwrap();
        assert_eq!(t.arg_names, vec!["t", "p"]);
        assert!(t.x_log && t.y_log);
        assert_eq!(t.curves.len(), 2);
        assert_eq!(t.curves[0].param, Some(100.0));
        assert_eq!(t.curves[0].xs, vec![1.0, 10.0]);
        assert_eq!(t.curves[0].ys, vec![2.0, 4.0]);
        assert_eq!(t.curves[1].param, Some(200.0));
        assert_eq!(t.curves[1].ys, vec![3.0, 5.0]);
        // [C] grounds to K; the family parameter's unit slot is None.
        assert_eq!(
            t.arg_units,
            Some(vec![Some("K".to_string()), None]),
            "argument unit + unannotated family slot"
        );
        assert_eq!(t.output_unit.as_deref(), Some("J/kg-K"));
    }

    #[test]
    fn ragged_table_rows_omit_later_columns() {
        // buildCurves: a row shorter than the column count contributes to the
        // earlier curves only.
        let doc = ok_real("TABLE cp(T : P = 1, 2)\n  1 10 20\n  2 11\nEND");
        let t = doc.defs.table("cp").unwrap();
        assert_eq!(t.curves[0].xs, vec![1.0, 2.0]);
        assert_eq!(t.curves[0].ys, vec![10.0, 11.0]);
        assert_eq!(t.curves[1].xs, vec![1.0]);
        assert_eq!(t.curves[1].ys, vec![20.0]);
    }

    #[test]
    fn a_family_count_mismatch_is_rejected_with_the_java_message() {
        let message = err_real("TABLE cp(T : P = 100, 200, 300)\n  1 2 3\nEND");
        assert!(
            message.contains("header declares 3 curve parameter value(s)")
                && message.contains("2 value column(s)"),
            "got: {message}"
        );
    }

    #[test]
    fn an_unknown_table_flag_names_the_supported_ones() {
        let message = err_real("TABLE t(x) SUPERLOG\n  1 2\nEND");
        assert!(
            message.contains("Unknown TABLE flag 'SUPERLOG'") && message.contains("XLOG, YLOG"),
            "got: {message}"
        );
    }

    #[test]
    fn a_table_needs_at_least_one_row() {
        let message = err_real("TABLE t(x)\nEND");
        assert!(message.contains("expected a number"), "got: {message}");
    }

    #[test]
    fn unterminated_definition_blocks_name_the_missing_end() {
        assert!(err_real("FUNCTION f(x)\n  f := x\n").contains("unterminated FUNCTION block"));
        assert!(err_real("PROCEDURE p(x : y)\n  y := x\n").contains("unterminated PROCEDURE"));
        assert!(err_real("MODULE m(x : y)\n  y = x\n").contains("unterminated MODULE"));
        assert!(err_real("TABLE t(x)\n  1 2\n").contains("unterminated TABLE"));
        assert!(err_real("FUNCTION f(x)\n  REPEAT\n    f := 1\n").contains("expected `UNTIL`"));
    }

    #[test]
    fn definition_headers_require_a_line_break() {
        assert!(err_real("FUNCTION f(x) f := x END").contains("after the FUNCTION header"));
        assert!(err_real("FUNCTION f(x)\n  IF x > 0 THEN f := 1 END\nEND").contains("after THEN"));
    }

    #[test]
    fn assignment_is_a_procedural_form_only() {
        // `:=` at the top level is not a statement; the equation grammar
        // rejects it where ANTLR would.
        let message = err_real("x := 1");
        assert!(message.contains("expected `=`"), "got: {message}");
    }

    #[test]
    fn a_later_definition_replaces_an_earlier_one_across_kinds() {
        // Java keys all defs in one map: MODULE f shadows FUNCTION f.
        let doc = ok_real(
            "FUNCTION f(x)\n  f := x\nEND\nMODULE f(a : b)\n  b = a\nEND\nFUNCTION g(x)\n  g := x\nEND\nFUNCTION g(y)\n  g := y + 1\nEND",
        );
        assert!(doc.defs.function("f").is_none(), "shadowed by the MODULE");
        assert!(doc.defs.module("f").is_some());
        assert_eq!(doc.defs.functions.len(), 1, "g replaced in place");
        assert_eq!(doc.defs.function("g").unwrap().params, vec!["y"]);
    }

    #[test]
    fn definitions_travel_alongside_statements_and_guesses() {
        let doc = ok_real(
            "GUESS y = 2\nFUNCTION Half(x)\n  Half := x / 2\nEND\ny = Half(10)\nPROCEDURE p(a : b)\n  b := a\nEND\nCALL p(1 : w)",
        );
        assert_eq!(doc.guesses.len(), 1);
        assert_eq!(doc.statements.len(), 2);
        assert_eq!(doc.defs.functions.len(), 1);
        assert_eq!(doc.defs.procedures.len(), 1);
        assert!(!doc.defs.is_empty());
    }

    // ── COMPONENT / instantiation / connect ─────────────────────────────────
    //
    // Like the definition blocks above, these go through `parse_document`: a
    // component body is full-fat expression territory (dotted port members,
    // property calls with named arguments).

    /// The worked example from `Frees.g4`'s own `componentDef` comment.
    #[test]
    fn a_component_definition_parses_ports_params_and_body() {
        let doc = ok_real(
            "COMPONENT Pump(in, out)\n\
             \x20 PARAM eta = 0.7, fluid$ = Water\n\
             \x20 v = Volume(fluid$, P=in.P, h=in.h)\n\
             \x20 out.mdot = in.mdot\n\
             \x20 out.h    = in.h + v*(out.P - in.P)/eta\n\
             \x20 W        = in.mdot*(out.h - in.h)\n\
             END",
        );
        assert!(doc.statements.is_empty(), "a COMPONENT is not a statement");
        assert_eq!(doc.components.defs.len(), 1);

        let def = doc.components.def("pump").expect("lowercased name");
        assert_eq!(def.ports, vec!["in", "out"]);
        assert_eq!(
            def.params
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["eta", "fluid$"]
        );
        assert_eq!(
            def.param("eta").unwrap().default_value,
            Some(Expr::num(0.7))
        );
        assert!(!def.param("eta").unwrap().is_string);
        // A `$` default is an ordinary expression — a bare fluid name is a Var.
        assert_eq!(
            def.param("fluid$").unwrap().default_value,
            Some(Expr::var("water"))
        );
        assert!(def.param("fluid$").unwrap().is_string);

        assert_eq!(def.body.len(), 4);
        // Port members survive whole, so the expander can rewrite them.
        assert_eq!(def.body[1].lhs, Expr::var("out.mdot"));
        assert_eq!(def.body[1].rhs, Expr::var("in.mdot"));
        // Diagnostics quote the user's line verbatim.
        assert_eq!(def.body[1].source_text, "out.mdot = in.mdot");
        assert!(def.variants.is_empty());
        assert!(!def.is_hierarchical());
    }

    #[test]
    fn a_parameter_may_be_declared_without_a_default() {
        // The standard library's rule — "no defaults, every parameter is
        // required" — is a *library* convention enforced at expansion time.
        // The grammar's `(EQ expr)?` is optional, and the parser records
        // exactly what was written.
        let doc = ok_real("COMPONENT Pump(in, out)\n  PARAM eta, fluid$\n  out.h = in.h\nEND");
        let def = doc.components.def("pump").unwrap();
        assert_eq!(def.params.len(), 2);
        assert!(def.params.iter().all(|p| p.default_value.is_none()));
        assert_eq!(
            def.params.iter().map(|p| p.is_string).collect::<Vec<_>>(),
            vec![false, true]
        );
    }

    #[test]
    fn several_param_lines_accumulate_in_declaration_order() {
        let doc = ok_real(
            "COMPONENT C(a)\n  PARAM x = 1, y\n  a.T = x\n  PARAM z$ = Water\n  a.P = y\nEND",
        );
        let def = doc.components.def("c").unwrap();
        assert_eq!(
            def.params
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["x", "y", "z$"]
        );
        assert_eq!(def.body.len(), 2, "PARAM lines are not equations");
    }

    #[test]
    fn variants_carry_their_require_list_and_body() {
        let doc = ok_real(
            "COMPONENT Compressor(in, out)\n\
             \x20 PARAM eta, model$ = isentropic\n\
             \x20 out.mdot = in.mdot\n\
             \x20 VARIANT isentropic REQUIRE eta\n\
             \x20   out.h = in.h + 1/eta\n\
             \x20 END\n\
             \x20 VARIANT map REQUIRE map_eta$, rpm\n\
             \x20   out.h = in.h + rpm\n\
             \x20 END\n\
             END",
        );
        let def = doc.components.def("compressor").unwrap();
        // The body outside every VARIANT is shared.
        assert_eq!(def.body.len(), 1);
        assert_eq!(def.variants.len(), 2);

        let isentropic = def.variant("isentropic").unwrap();
        assert_eq!(isentropic.require, vec!["eta"]);
        assert_eq!(isentropic.body.len(), 1);

        // `AstBuilder.buildComponentDef` promotes every REQUIRE name that is
        // not already a PARAM into a defaultless parameter, `$` and all.
        assert_eq!(
            def.params
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["eta", "model$", "map_eta$", "rpm"]
        );
        assert!(def.param("map_eta$").unwrap().is_string);
        assert_eq!(def.param("map_eta$").unwrap().default_value, None);
        assert_eq!(
            def.param("eta").unwrap().default_value,
            None,
            "not clobbered"
        );
    }

    #[test]
    fn a_variant_needs_no_require_clause() {
        let doc = ok_real(
            "COMPONENT V(p)\n  PARAM model$ = basic\n  VARIANT basic\n    p.T = 1\n  END\nEND",
        );
        let def = doc.components.def("v").unwrap();
        assert_eq!(def.variant("basic").unwrap().require, Vec::<String>::new());
        assert_eq!(def.variant("basic").unwrap().body.len(), 1);
        assert!(def.body.is_empty());
    }

    #[test]
    fn an_instantiation_binds_ports_positionally_then_overrides_params() {
        let doc = ok_real("Pump P1(s3, s4, eta=0.8, fluid$=Water)");
        assert!(doc.statements.is_empty(), "not an equation");
        assert_eq!(doc.components.instances.len(), 1);

        let inst = doc.components.instance("p1").expect("lowercased name");
        assert_eq!(inst.type_name, "pump");
        assert_eq!(inst.port_args, vec!["s3", "s4"]);
        assert_eq!(
            inst.params.iter().map(|(k, _)| k).collect::<Vec<_>>(),
            vec!["eta", "fluid$"]
        );
        assert_eq!(inst.params.get("eta"), Some(&Expr::num(0.8)));
        assert_eq!(inst.params.get("fluid$"), Some(&Expr::var("water")));
        assert_eq!(inst.source_text, "Pump P1(s3, s4, eta=0.8, fluid$=Water)");
    }

    #[test]
    fn an_instantiation_may_take_no_arguments_at_all() {
        // `TransGround G()` in the shipped mechanical library.
        let doc = ok_real("TransGround G()");
        let inst = doc.components.instance("g").unwrap();
        assert!(inst.port_args.is_empty());
        assert!(inst.params.is_empty());
    }

    #[test]
    fn a_named_argument_takes_a_full_expression() {
        let doc = ok_real("HeatExchanger C1(UA=UA/2, hot$=hot$, arr$=arr$)");
        let inst = doc.components.instance("c1").unwrap();
        assert!(inst.port_args.is_empty());
        assert_eq!(
            inst.params.get("ua"),
            Some(&Expr::bin(BinOp::Div, Expr::var("ua"), Expr::num(2.0)))
        );
        assert_eq!(inst.params.get("hot$"), Some(&Expr::var("hot$")));
    }

    #[test]
    fn a_positional_argument_after_a_named_one_is_rejected() {
        let message = err_real("Pump P1(eta=0.8, s3)");
        assert!(
            message.contains("positional port arguments must come before"),
            "got: {message}"
        );
        assert!(
            message.contains("P1") || message.contains("p1"),
            "{message}"
        );
    }

    #[test]
    fn a_port_argument_that_is_not_a_stream_name_is_rejected() {
        let message = err_real("Pump P1(s3 + 1, s4)");
        assert!(
            message.contains("each port argument must be a stream name"),
            "got: {message}"
        );
        // The rejection quotes what the user wrote.
        assert!(message.contains("s3 + 1"), "got: {message}");
        assert!(err_real("Pump P1(3)").contains("must be a stream name"));
    }

    #[test]
    fn a_repeated_named_argument_overwrites_in_place() {
        // `LinkedHashMap.put`: last value, first slot.
        let inst = ok_real("Pump P1(a, eta=1, fluid$=Water, eta=2)")
            .components
            .instances
            .remove(0);
        assert_eq!(
            inst.params.iter().map(|(k, _)| k).collect::<Vec<_>>(),
            vec!["eta", "fluid$"]
        );
        assert_eq!(inst.params.get("eta"), Some(&Expr::num(2.0)));
    }

    #[test]
    fn connect_keeps_its_dotted_endpoints_lowercased() {
        let doc = ok_real("connect(HP.out, LP.in, F1.steam, bare)");
        assert!(doc.statements.is_empty());
        assert_eq!(doc.components.connects.len(), 1);
        let connect = &doc.components.connects[0];
        assert_eq!(connect.ports, vec!["hp.out", "lp.in", "f1.steam", "bare"]);
        assert_eq!(
            connect.source_text,
            "connect(HP.out, LP.in, F1.steam, bare)"
        );
        // An endpoint is a name, not an expression: it never registers a
        // display name (`AstBuilder.buildConnect` reads the IDENT tokens).
        assert!(!doc.display_names.contains_key("hp"));
        assert!(!doc.display_names.contains_key("bare"));
    }

    /// `connectPort (COMMA connectPort)*` admits a lone endpoint, and the
    /// parser must let it through: the Java raises *"connect(...) needs at
    /// least two endpoints"* from `ComponentExpander.expandConnects`, not from
    /// the builder. Verified against the oracle — refusing it here would fail a
    /// document one stage earlier than the reference engine does.
    #[test]
    fn a_single_endpoint_connect_parses_and_is_the_expanders_problem() {
        let doc = ok_real("connect(s1)");
        assert_eq!(doc.components.connects[0].ports, vec!["s1"]);
        // Zero endpoints is a *grammar* error, though — the rule requires one.
        assert!(err_real("connect()").contains("expected an identifier"));
    }

    /// A positional port argument may itself be dotted: `Expr.Var` covers a
    /// bare stream name and a dotted path alike, and the Java binds the port to
    /// the dotted name verbatim (oracle: `Probe p1(s1.x)` solves `s1.x.t`).
    #[test]
    fn a_positional_port_argument_may_be_a_dotted_path() {
        let doc = ok_real("Probe p1(s1.x, s2)");
        assert_eq!(doc.components.instances[0].port_args, vec!["s1.x", "s2"]);
    }

    #[test]
    fn a_hierarchical_component_holds_sub_instances_and_internal_connects() {
        let doc = ok_real(
            "COMPONENT Chiller(ref_in, ref_out, cool_in, cool_out)\n\
             \x20 PARAM ref$, cool$, UA_cool\n\
             \x20 MovingBoundaryEvaporator EV(fluid$=ref$)\n\
             \x20 LiquidWallHX CL(fluid$=cool$, UA=UA_cool)\n\
             \x20 connect(ref_in, EV.in)\n\
             \x20 connect(EV.out, ref_out)\n\
             \x20 connect(EV.wall, CL.wall)\n\
             END",
        );
        let def = doc.components.def("chiller").unwrap();
        assert!(def.is_hierarchical());
        assert!(def.body.is_empty(), "a subsystem carries no equations here");
        assert_eq!(
            def.sub_instances
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>(),
            vec!["ev", "cl"]
        );
        assert_eq!(def.sub_instances[0].type_name, "movingboundaryevaporator");
        assert_eq!(
            def.sub_instances[1].params.get("ua"),
            Some(&Expr::var("ua_cool"))
        );
        assert_eq!(def.sub_connects.len(), 3);
        assert_eq!(def.sub_connects[2].ports, vec!["ev.wall", "cl.wall"]);
        // Sub-instances and sub-connects belong to the definition, not to the
        // document's own top-level lists.
        assert!(doc.components.instances.is_empty());
        assert!(doc.components.connects.is_empty());
    }

    /// `componentItem` has five alternatives and they may appear in any order
    /// and any number — the dispatch must not depend on position.
    #[test]
    fn all_five_component_item_kinds_mix_freely_in_one_body() {
        let doc = ok_real(
            "COMPONENT Mixed(a, b)\n\
             \x20 a.T = 1\n\
             \x20 PARAM k, model$ = one\n\
             \x20 Sub S1(a, k=k)\n\
             \x20 VARIANT one REQUIRE k\n\
             \x20   b.T = a.T * k\n\
             \x20 END\n\
             \x20 connect(a, S1.p)\n\
             \x20 PARAM j\n\
             \x20 b.P = a.P\n\
             END",
        );
        let def = doc.components.def("mixed").unwrap();
        assert_eq!(def.body.len(), 2, "the two equations outside the variant");
        assert_eq!(
            def.params
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["k", "model$", "j"],
            "PARAM lines accumulate wherever they appear; REQUIRE k is already declared"
        );
        assert_eq!(def.variants.len(), 1);
        assert_eq!(def.sub_instances.len(), 1);
        assert_eq!(def.sub_connects.len(), 1);
        assert_eq!(def.sub_instances[0].port_args, vec!["a"]);
        assert_eq!(def.sub_connects[0].ports, vec!["a", "s1.p"]);
    }

    #[test]
    fn components_travel_alongside_statements_defs_and_guesses() {
        let doc = ok_real(
            "GUESS mdot = 0.5\n\
             P_in = 1e5\n\
             COMPONENT Pump(in, out)\n  PARAM eta\n  out.h = in.h/eta\nEND\n\
             Pump P1(s1, s2, eta=0.8)\n\
             connect(P1.out, s3)\n",
        );
        assert_eq!(doc.guesses.len(), 1);
        assert_eq!(doc.statements.len(), 1);
        assert_eq!(doc.components.defs.len(), 1);
        assert_eq!(doc.components.instances.len(), 1);
        assert_eq!(doc.components.connects.len(), 1);
        assert!(!doc.components.is_empty());
    }

    #[test]
    fn two_definitions_of_one_name_both_survive_the_parse() {
        // "Two user definitions collide" is a `ComponentExpander` error, not a
        // parser one — the parser must not silently drop either.
        let doc = ok_real("COMPONENT P(a)\n  a.T = 1\nEND\nCOMPONENT P(a, b)\n  a.T = b.T\nEND");
        assert_eq!(doc.components.defs.len(), 2);
        assert_eq!(
            doc.components.def("p").unwrap().ports.len(),
            1,
            "first wins"
        );
    }

    #[test]
    fn an_unterminated_component_block_names_the_block() {
        assert!(err_real("COMPONENT P(a)\n  a.T = 1\n").contains("unterminated COMPONENT p"));
        assert!(err_real("COMPONENT P(a)\n  VARIANT v\n    a.T = 1\n")
            .contains("unterminated VARIANT v"));
    }

    #[test]
    fn a_component_header_must_be_followed_by_a_line_break() {
        let message = err_real("COMPONENT P(a) a.T = 1 END");
        assert!(
            message.contains("after the COMPONENT header"),
            "got: {message}"
        );
        assert!(err_real("COMPONENT P\n  a = 1\nEND").contains("expected `(`"));
    }

    /// `componentDef` spells its body `componentItem (sep componentItem)* sep?
    /// END` while `componentVariant` spells it `(equation sep)* END` — the
    /// separator before `END` is optional in the first and mandatory in the
    /// second, and the port keeps that distinction.
    #[test]
    fn the_separator_before_end_follows_each_rule_exactly() {
        assert!(parse_document("COMPONENT P(a)\n  a.T = 1 END").is_ok());
        let message = err_real("COMPONENT P(a)\n  VARIANT v\n    a.T = 1 END\nEND");
        assert!(
            message.contains("after a VARIANT equation"),
            "got: {message}"
        );
    }

    #[test]
    fn a_component_body_needs_at_least_one_item() {
        // The grammar demands `componentItem+`; an immediate END earns the
        // natural expression error, as `TABLE` does for an empty body.
        let message = err_real("COMPONENT P(a)\nEND");
        assert!(message.contains("END"), "got: {message}");
    }

    /// `statementList` does not admit `componentInst`, so the shape stays a
    /// syntax error inside a `FOR` body — exactly where ANTLR reports one.
    #[test]
    fn an_instantiation_inside_a_for_body_is_a_syntax_error() {
        let message = err_real("FOR i = 1 TO 2\n  Pump P1(s1, s2)\nEND");
        assert!(!message.contains("not supported"), "got: {message}");
        assert!(message.contains("expected `=`"), "got: {message}");
    }

    #[test]
    fn a_two_identifier_shape_without_parens_is_still_an_equation_error() {
        // `at_component_inst` needs all three tokens; `Pump P1 = 2` is not one.
        assert!(err_real("Pump P1 = 2").contains("expected `=`"));
    }

    /// Which parts of the component grammar feed `ParseResult.displayNames` is
    /// not guessable — it follows exactly which sub-trees `AstBuilder` runs
    /// `visit` over. Established against the Java oracle: solving
    ///
    /// ```text
    /// COMPONENT Probe(a)
    ///   PARAM model$ = basic
    ///   VARIANT basic
    ///     a.T = 1
    ///   END
    /// END
    /// Probe p1(s1)
    /// ```
    ///
    /// produced `display_names = {basic: basic, s1: s1, s1$t: s1.t}`, of which
    /// `s1$t` is added later by the expander. So a `PARAM` default expression
    /// and a positional port argument register (they are visited as
    /// expressions); port names, parameter names, variant names, `REQUIRE`
    /// names, the instance and its type, a dotted body member and a `connect`
    /// endpoint do not.
    #[test]
    fn display_names_come_only_from_the_visited_expressions() {
        let doc = ok_real(
            "COMPONENT Probe(a)\n\
             \x20 PARAM model$ = basic\n\
             \x20 VARIANT basic\n\
             \x20   a.T = 1\n\
             \x20 END\n\
             END\n\
             Probe p1(s1)",
        );
        assert_eq!(
            doc.display_names.keys().collect::<Vec<_>>(),
            vec!["basic", "s1"]
        );
        assert_eq!(doc.display_names.get("s1"), Some(&"s1".to_string()));
    }
}
