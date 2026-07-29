//! Statement and top-level grammar.
//!
//! Rules `program`, `topLevel`, `statement`, `statementList`, `forBlock`,
//! `callStatement`, `multiAssign`, `rangeAssign`, `symbolicDecl`,
//! `guessDirective`, `equation` from `Frees.g4`.
//!
//! Port of `AstBuilder.buildProgram` / `buildStatement` and friends
//! (`../frEES/backend/core/src/main/java/com/frees/backend/parser/AstBuilder.java`).
//!
//! # What this pass covers
//!
//! `topLevel` in the grammar admits eleven block constructs on top of the plain
//! statement forms. Only `guessDirective` and `statement` are implemented here;
//! every other leading token produces [`crate::parser::unsupported`] naming the
//! construct, so a document that uses one fails loudly instead of being
//! silently mis-parsed (`Document`'s doc comment: "a wrong answer is worse than
//! a refusal").
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
//! * **`componentInst`** is `IDENT IDENT (`. It is not implemented, but it is
//!   detected, because letting it fall through to `equation` would report a
//!   confusing expression error instead of "not supported".
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
use crate::diag::{FreesError, Result, Span};
use crate::parser::{unsupported, Cursor, Document, GuessDirective};
use crate::token::{Token, TokenKind};

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
        Ok(doc)
    }

    /// `topLevel`. Only `guessDirective` and `statement` are supported; every
    /// block form is rejected by [`Parser::statement`].
    fn top_level(&mut self, doc: &mut Document) -> Result<()> {
        if matches!(self.c.peek(), TokenKind::Guess) {
            let directive = self.guess_directive()?;
            doc.guesses.push(directive);
        } else {
            let statement = self.statement()?;
            doc.statements.push(statement);
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
        if self.at_component_inst() {
            return Err(unsupported("COMPONENT instantiation", self.c.span()));
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
        if self.block_depth >= MAX_BLOCK_DEPTH {
            return Err(FreesError::parse_at(
                format!("blocks are nested more than {MAX_BLOCK_DEPTH} levels deep"),
                header,
            ));
        }
        self.block_depth += 1;
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
        if !self.c.skip_separators() {
            return Err(FreesError::parse_at(
                format!(
                    "expected a line break or `;` after the FOR header, found {}",
                    self.c.peek().describe()
                ),
                self.c.span(),
            ));
        }

        // `statementList sep? END`
        let mut body = Vec::new();
        loop {
            self.c.skip_separators();
            if matches!(self.c.peek(), TokenKind::End) {
                break;
            }
            if self.c.is_eof() {
                return Err(FreesError::parse_at(
                    "unterminated FOR block: expected `END`",
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
        self.c.expect(&TokenKind::End)?;

        Ok(Statement::For {
            var_name,
            start,
            end,
            body,
        })
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
        let start_pos = self.c.pos();
        let lhs = self.expr()?;
        self.c.expect(&TokenKind::Eq)?;
        let rhs = self.expr()?;
        Ok(Statement::Eq(Equation::new(
            lhs,
            rhs,
            self.text_since(start_pos),
        )))
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

    // ── shared pieces ───────────────────────────────────────────────────────

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
    /// test is exact.
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
/// keyed by their leading token.
fn unsupported_construct(kind: &TokenKind) -> Option<&'static str> {
    Some(match kind {
        TokenKind::Function => "FUNCTION",
        TokenKind::Procedure => "PROCEDURE",
        TokenKind::Module => "MODULE",
        TokenKind::Table => "TABLE",
        TokenKind::Parametric => "PARAMETRIC",
        TokenKind::StateTable => "STATE TABLE",
        TokenKind::Plot => "PLOT",
        TokenKind::Dynamic => "DYNAMIC",
        TokenKind::Linearize => "LINEARIZE",
        TokenKind::Component => "COMPONENT",
        TokenKind::Connect => "CONNECT",
        _ => return None,
    })
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
            ("FUNCTION f(x)\n  f := x\nEND", "FUNCTION"),
            ("PROCEDURE p(x : y)\n  y := x\nEND", "PROCEDURE"),
            ("MODULE m(x : y)\n  y = x\nEND", "MODULE"),
            ("TABLE t(x)\n  1 2\nEND", "TABLE"),
            ("PARAMETRIC sweep(a)\n  a = 1:2:3\nEND", "PARAMETRIC"),
            (
                "STATE TABLE circuit(P1)\n  FLUID = Water\nEND",
                "STATE TABLE",
            ),
            ("PLOT 'speed'\n  kind = xy\nEND", "PLOT"),
            ("DYNAMIC d(method = ode45)\n  der = 1\nEND", "DYNAMIC"),
            ("LINEARIZE plant(block = w)\n  INPUT q\nEND", "LINEARIZE"),
            ("COMPONENT Pump(in, out)\n  a = 1\nEND", "COMPONENT"),
            ("connect(HP.out, LP.in)", "CONNECT"),
        ];
        for (src, construct) in cases {
            let message = err(src);
            assert!(
                message.contains(construct) && message.contains("not supported"),
                "`{construct}` should be refused by name, got: {message}"
            );
        }
    }

    #[test]
    fn a_component_instantiation_is_detected_rather_than_mis_parsed() {
        let message = err("Pump P1(s3, s4, eta=0.8)");
        assert!(
            message.contains("COMPONENT instantiation") && message.contains("not supported"),
            "got: {message}"
        );
    }

    #[test]
    fn unsupported_errors_are_anchored_to_the_offending_token() {
        let src = "x = 1\nMODULE m(a : b)\n  b = a\nEND";
        let error = parse(src).unwrap_err();
        let span = error.span().expect("an unsupported error carries a span");
        assert_eq!(span.slice(src), "MODULE");
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

        let error = parse_document("MODULE m(a : b)\n  b = a\nEND").unwrap_err();
        assert!(
            error.to_string().contains("MODULE") && error.to_string().contains("not supported"),
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
}
