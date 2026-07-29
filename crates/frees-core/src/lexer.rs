//! Lexer — source text to [`Token`]s.
//!
//! Port of the lexer half of `Frees.g4` (lines 499-632). All the ordering and
//! longest-match subtleties are documented on [`crate::token::TokenKind`];
//! read those before changing this file.
//!
//! # How the ANTLR lexer is emulated
//!
//! ANTLR's lexer is a maximal-munch DFA over *all* rules: at every position it
//! takes the **longest** rule match, breaking ties by declaration order. The
//! scanner below is a hand-written equivalent, so each place where two rules
//! could both match is resolved explicitly:
//!
//! | Source | ANTLR candidates | Winner |
//! |---|---|---|
//! | `..`   | `DOTDOT`(2) vs `DOT`(1)            | `DOTDOT` |
//! | `.5`   | `NUMBER`(2) vs `DOT`(1)            | `NUMBER` |
//! | `.*`   | `DOTSTAR`(2) vs `DOT`(1)           | `DOTSTAR` |
//! | `:=`   | `ASSIGN`(2) vs `COLON`(1)          | `ASSIGN` |
//! | `<=`   | `LE`(2) vs `LT`(1)                 | `LE` |
//! | `->`   | `ARROW`(2) vs `MINUS`(1)           | `ARROW` |
//! | `2i`   | `IMAG_NUMBER`(2) vs `NUMBER`(1)    | `IMAG_NUMBER` |
//! | `STATE TABLE` | `STATETABLE`(11) vs `IDENT`(5) | `STATETABLE` |
//! | `'…'`  | `STRING_LITERAL`(n) vs `TRANSPOSE`(1) | `STRING_LITERAL` |
//!
//! Keywords are matched by scanning a whole identifier first and classifying it
//! afterwards ([`TokenKind::keyword_or_ident`]) — which is what ANTLR's maximal
//! munch does, and why `format` is an identifier rather than `FOR` + `mat`.

use crate::diag::{FreesError, Result, Span};
use crate::token::{Token, TokenKind};

/// Tokenize a whole document.
///
/// Comments (`{…}`, `"…"`, `//…`) and horizontal whitespace are skipped. Line
/// breaks are emitted as a single collapsed [`crate::token::TokenKind::Newline`].
/// The stream always ends with [`crate::token::TokenKind::Eof`].
pub fn tokenize(source: &str) -> Result<Vec<Token>> {
    Lexer::new(source).run()
}

/// Span of the whole document, for errors with no better anchor.
pub fn document_span(source: &str) -> Span {
    Span::new(0, source.len())
}

/// A byte-oriented scanner. Every construct in the grammar is ASCII, so the
/// scanner walks bytes and only decodes UTF-8 when it has to quote an offending
/// character in a diagnostic. `pos` is therefore always on a char boundary:
/// the only way to reach a non-ASCII byte is the error path, which never
/// advances past it.
struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Lexer<'a> {
        Lexer {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            tokens: Vec::new(),
        }
    }

    // ── Cursor primitives ───────────────────────────────────────────────────

    #[inline]
    fn byte(&self, at: usize) -> Option<u8> {
        self.bytes.get(at).copied()
    }

    #[inline]
    fn cur(&self) -> Option<u8> {
        self.byte(self.pos)
    }

    #[inline]
    fn peek(&self, n: usize) -> Option<u8> {
        self.byte(self.pos + n)
    }

    /// Emit a token spanning `start .. self.pos`.
    fn push(&mut self, kind: TokenKind, start: usize) {
        self.tokens
            .push(Token::new(kind, Span::new(start, self.pos)));
    }

    // ── Driver ──────────────────────────────────────────────────────────────

    fn run(mut self) -> Result<Vec<Token>> {
        // A UTF-8 BOM is not in the grammar; Windows-authored documents carry
        // one anyway. Skipping it beats failing on byte 0 (see module docs).
        if self.src.starts_with('\u{feff}') {
            self.pos = '\u{feff}'.len_utf8();
        }
        loop {
            self.skip_trivia()?;
            let start = self.pos;
            let b = match self.cur() {
                Some(b) => b,
                None => break,
            };
            match b {
                b'\r' | b'\n' => self.scan_newline(start)?,
                b'0'..=b'9' => self.scan_number(start)?,
                b'.' => self.scan_dot(start)?,
                b'\'' => self.scan_quote(start),
                b'a'..=b'z' | b'A'..=b'Z' => self.scan_word(start),
                _ => self.scan_operator(start)?,
            }
        }
        let end = self.src.len();
        self.tokens
            .push(Token::new(TokenKind::Eof, Span::new(end, end)));
        Ok(self.tokens)
    }

    // ── Trivia: whitespace and the three comment forms ──────────────────────

    /// Skip `[ \t]+`, `{ … }`, `" … "` and `// …` until real input is reached.
    ///
    /// Both delimited comment forms are non-greedy (`.*?`), i.e. they end at
    /// the *first* closing delimiter, and — as in ANTLR — `.` matches line
    /// breaks, so a delimited comment may span lines and swallow the newlines
    /// inside it.
    fn skip_trivia(&mut self) -> Result<()> {
        loop {
            match self.cur() {
                Some(b' ') | Some(b'\t') => self.pos += 1,
                Some(b'{') => self.skip_delimited(b'{', '}')?,
                // NB: a double quote opens a COMMENT in frees. String literals
                // are single-quoted (see `scan_quote`).
                Some(b'"') => self.skip_delimited(b'"', '"')?,
                Some(b'/') if self.peek(1) == Some(b'/') => {
                    let rest = &self.src[self.pos..];
                    // LINE_COMMENT : '//' ~[\r\n]* — the line break itself is
                    // left in place so it still produces a NEWLINE token.
                    self.pos += rest.find(['\r', '\n']).unwrap_or(rest.len());
                }
                _ => return Ok(()),
            }
        }
    }

    fn skip_delimited(&mut self, open: u8, close: char) -> Result<()> {
        let start = self.pos;
        match self.src[start + 1..].find(close) {
            Some(off) => {
                self.pos = start + 1 + off + close.len_utf8();
                Ok(())
            }
            None => Err(FreesError::parse_at(
                format!(
                    "unterminated comment: no closing `{close}` for this `{}`",
                    open as char
                ),
                Span::new(start, start + 1),
            )),
        }
    }

    // ── NEWLINE : ('\r'? '\n')+ ─────────────────────────────────────────────

    /// Collapse a run of line breaks into one token.
    ///
    /// Only `\r\n` and bare `\n` are line breaks; anything between two breaks
    /// (even a single space) ends the run, exactly as the ANTLR rule does.
    fn scan_newline(&mut self, start: usize) -> Result<()> {
        loop {
            match self.cur() {
                Some(b'\n') => self.pos += 1,
                Some(b'\r') if self.peek(1) == Some(b'\n') => self.pos += 2,
                _ => break,
            }
        }
        if self.pos == start {
            // A lone '\r' (classic-Mac line ending) matches no lexer rule.
            return Err(FreesError::parse_at(
                "stray carriage return: frees line endings are `\\n` or `\\r\\n`",
                Span::new(start, start + 1),
            ));
        }
        self.push(TokenKind::Newline, start);
        Ok(())
    }

    // ── NUMBER / IMAG_NUMBER ────────────────────────────────────────────────

    /// `DIGIT+ ('.' DIGIT+)? EXPONENT? [iIjJ]?` and the leading-dot form
    /// `'.' DIGIT+ EXPONENT? [iIjJ]?`.
    ///
    /// Called with the cursor on a digit, or on a `.` that is followed by a
    /// digit — the two grammar alternatives share one scanner.
    fn scan_number(&mut self, start: usize) -> Result<()> {
        while matches!(self.cur(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        // A fraction needs at least one digit after the dot, so `1.` is
        // NUMBER(1) + DOT and `A.b` never looks numeric.
        if self.cur() == Some(b'.') && matches!(self.peek(1), Some(b'0'..=b'9')) {
            self.pos += 1;
            while matches!(self.cur(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        self.scan_exponent();
        let digits_end = self.pos;
        let digits = &self.src[start..digits_end];
        let value = digits.parse::<f64>().map_err(|_| {
            FreesError::parse_at(
                format!("invalid number literal `{digits}`"),
                Span::new(start, digits_end),
            )
        })?;

        // IMAG_NUMBER is NUMBER + [iIjJ]; it is longer, so it wins.
        if matches!(self.cur(), Some(b'i' | b'I' | b'j' | b'J')) {
            self.pos += 1;
            let text = self.src[start..self.pos].to_string();
            self.push(TokenKind::ImagNumber { value, text }, start);
        } else {
            let text = digits.to_string();
            self.push(TokenKind::Number { value, text }, start);
        }
        Ok(())
    }

    /// `EXPONENT : [eE] [+-]? DIGIT+`, consumed only when it is complete.
    ///
    /// A dangling `e` is not an error — `3e` is NUMBER(3) + IDENT(e), because
    /// the ANTLR alternative simply fails and the shorter NUMBER match wins.
    fn scan_exponent(&mut self) {
        if !matches!(self.cur(), Some(b'e' | b'E')) {
            return;
        }
        let mut p = self.pos + 1;
        if matches!(self.byte(p), Some(b'+' | b'-')) {
            p += 1;
        }
        if !matches!(self.byte(p), Some(b'0'..=b'9')) {
            return; // backtrack: leave the cursor before the `e`
        }
        while matches!(self.byte(p), Some(b'0'..=b'9')) {
            p += 1;
        }
        self.pos = p;
    }

    // ── The '.' family ──────────────────────────────────────────────────────

    fn scan_dot(&mut self, start: usize) -> Result<()> {
        let (kind, len) = match self.peek(1) {
            Some(b'.') => (TokenKind::DotDot, 2),
            Some(b'*') => (TokenKind::DotStar, 2),
            Some(b'/') => (TokenKind::DotSlash, 2),
            Some(b'\\') => (TokenKind::DotBackslash, 2),
            Some(b'^') => (TokenKind::DotCaret, 2),
            Some(b'0'..=b'9') => return self.scan_number(start),
            _ => (TokenKind::Dot, 1),
        };
        self.pos = start + len;
        self.push(kind, start);
        Ok(())
    }

    // ── STRING_LITERAL vs TRANSPOSE ─────────────────────────────────────────

    /// Resolve a single quote.
    ///
    /// `STRING_LITERAL : '\'' (~'\'')* '\''` and `TRANSPOSE : '\''` both start
    /// here. ANTLR takes the longest match: since `~'\''` cannot contain a
    /// quote, the string can only end at the *next* quote, so
    ///
    /// * if another `'` exists anywhere later in the document → STRING_LITERAL;
    /// * otherwise the string alternative cannot match at all → TRANSPOSE.
    ///
    /// **Tradeoff.** This reproduces the Java engine bit for bit, including its
    /// wart: `A' + B'` lexes as `IDENT(A)` + `STRING_LITERAL(" + B")`, not as
    /// two transposes. The alternative — deciding from the previous token,
    /// MATLAB-style (a quote right after an atom is a transpose) — reads better
    /// but would silently disagree with the Java oracle on documents that mix
    /// the two, which is exactly what the parity harness is there to catch. So
    /// the port stays faithful; a transpose is unambiguous whenever no later
    /// quote follows it, which is the case in single-quote-free documents.
    fn scan_quote(&mut self, start: usize) {
        match self.src[start + 1..].find('\'') {
            Some(off) => {
                let close = start + 1 + off;
                let content = self.src[start + 1..close].to_string();
                self.pos = close + 1;
                self.push(TokenKind::StringLiteral(content), start);
            }
            None => {
                self.pos = start + 1;
                self.push(TokenKind::Transpose, start);
            }
        }
    }

    // ── IDENT / keywords / STATE TABLE ──────────────────────────────────────

    /// `IDENT : [a-zA-Z] [a-zA-Z0-9_]* ('$'|'#')?`, then keyword classification.
    fn scan_word(&mut self, start: usize) {
        self.pos += 1;
        while matches!(
            self.cur(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
        ) {
            self.pos += 1;
        }
        let bare_end = self.pos;
        // A trailing sigil belongs to the identifier and disqualifies it as a
        // keyword — `keyword_or_ident` enforces that, and STATETABLE is checked
        // only for the sigil-free spelling.
        let has_sigil = matches!(self.cur(), Some(b'$' | b'#'));
        if has_sigil {
            self.pos += 1;
        }

        if !has_sigil && self.src[start..bare_end].eq_ignore_ascii_case("state") {
            if let Some(end) = self.state_table_tail(bare_end) {
                self.pos = end;
                self.push(TokenKind::StateTable, start);
                return;
            }
        }

        let kind = TokenKind::keyword_or_ident(&self.src[start..self.pos]);
        self.push(kind, start);
    }

    /// `STATETABLE : [sS][tT][aA][tT][eE] [ \t]+ [tT][aA][bB][lL][eE]` —
    /// lookahead past horizontal whitespace for the second word.
    ///
    /// Deliberately *not* checking for a word boundary after `table`: the ANTLR
    /// DFA does not either, so `state tables` is STATETABLE + IDENT(`s`). A
    /// lone `state`, `state$`, or `state` on its own line stays an identifier.
    fn state_table_tail(&self, after_state: usize) -> Option<usize> {
        let mut p = after_state;
        while matches!(self.byte(p), Some(b' ' | b'\t')) {
            p += 1;
        }
        if p == after_state {
            return None; // `[ \t]+` requires at least one separator
        }
        let tail = self.bytes.get(p..p + 5)?;
        if tail.eq_ignore_ascii_case(b"table") {
            Some(p + 5)
        } else {
            None
        }
    }

    // ── Operators and delimiters ────────────────────────────────────────────

    fn scan_operator(&mut self, start: usize) -> Result<()> {
        let b = match self.cur() {
            Some(b) => b,
            None => return Ok(()),
        };
        let (kind, len) = match b {
            b'=' => (TokenKind::Eq, 1),
            b'+' => (TokenKind::Plus, 1),
            b'-' => match self.peek(1) {
                Some(b'>') => (TokenKind::Arrow, 2),
                _ => (TokenKind::Minus, 1),
            },
            b'*' => (TokenKind::Times, 1),
            b'/' => (TokenKind::Div, 1),
            b'^' => (TokenKind::Caret, 1),
            b'\\' => (TokenKind::Backslash, 1),
            b'~' => (TokenKind::Tilde, 1),
            b'|' => (TokenKind::Pipe, 1),
            b':' => match self.peek(1) {
                Some(b'=') => (TokenKind::Assign, 2),
                _ => (TokenKind::Colon, 1),
            },
            b'<' => match self.peek(1) {
                Some(b'=') => (TokenKind::Le, 2),
                Some(b'>') => (TokenKind::Ne, 2),
                _ => (TokenKind::Lt, 1),
            },
            b'>' => match self.peek(1) {
                Some(b'=') => (TokenKind::Ge, 2),
                _ => (TokenKind::Gt, 1),
            },
            b'(' => (TokenKind::LParen, 1),
            b')' => (TokenKind::RParen, 1),
            b'[' => (TokenKind::LBracket, 1),
            b']' => (TokenKind::RBracket, 1),
            b';' => (TokenKind::Semi, 1),
            b',' => (TokenKind::Comma, 1),
            _ => return Err(self.unexpected(start)),
        };
        self.pos = start + len;
        self.push(kind, start);
        Ok(())
    }

    fn unexpected(&self, start: usize) -> FreesError {
        let ch = self.src[start..].chars().next().unwrap_or('\u{fffd}');
        let end = start + ch.len_utf8();
        let hint = match ch {
            '_' => " (an identifier must start with a letter)",
            '}' => " (no `{` opened a comment here)",
            '$' | '#' => " (a `$`/`#` sigil may only trail an identifier)",
            _ => "",
        };
        FreesError::parse_at(
            format!("unexpected character `{ch}`{hint}"),
            Span::new(start, end),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── helpers ─────────────────────────────────────────────────────────────

    fn lex(src: &str) -> Vec<Token> {
        tokenize(src).unwrap_or_else(|e| panic!("lexing {src:?} failed: {e}"))
    }

    /// Token kinds without the trailing `Eof`.
    fn kinds(src: &str) -> Vec<TokenKind> {
        let mut toks = lex(src);
        let last = toks.pop().expect("always at least Eof");
        assert_eq!(last.kind, TokenKind::Eof, "stream must end with Eof");
        toks.into_iter().map(|t| t.kind).collect()
    }

    fn err(src: &str) -> FreesError {
        tokenize(src).expect_err("expected a lex error")
    }

    fn ident(name: &str) -> TokenKind {
        TokenKind::Ident(name.to_string())
    }

    fn num(value: f64, text: &str) -> TokenKind {
        TokenKind::Number {
            value,
            text: text.to_string(),
        }
    }

    fn imag(value: f64, text: &str) -> TokenKind {
        TokenKind::ImagNumber {
            value,
            text: text.to_string(),
        }
    }

    fn string(s: &str) -> TokenKind {
        TokenKind::StringLiteral(s.to_string())
    }

    // ── structure ───────────────────────────────────────────────────────────

    #[test]
    fn empty_input_is_just_eof() {
        let toks = lex("");
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].kind, TokenKind::Eof);
        assert_eq!(toks[0].span, Span::new(0, 0));
    }

    #[test]
    fn whitespace_only_input_is_just_eof() {
        let toks = lex("   \t  ");
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].kind, TokenKind::Eof);
        assert_eq!(toks[0].span, Span::new(6, 6));
    }

    #[test]
    fn exactly_one_eof_at_the_end() {
        let toks = lex("a = 1\nb = 2\n");
        assert_eq!(toks.iter().filter(|t| t.kind == TokenKind::Eof).count(), 1);
        assert_eq!(toks.last().unwrap().kind, TokenKind::Eof);
    }

    #[test]
    fn every_span_slices_back_to_its_own_text() {
        let src = "T_in = 300 [K] ; h := Enthalpy(R134a, T=T_in)\nx' = .5e-3";
        for tok in lex(src) {
            let text = tok.span.slice(src);
            match &tok.kind {
                TokenKind::Eof => assert_eq!(text, ""),
                TokenKind::Newline => assert!(text.chars().all(|c| c == '\n' || c == '\r')),
                TokenKind::Ident(name) => assert_eq!(text, name),
                TokenKind::Number { text: t, .. } => assert_eq!(text, t),
                TokenKind::StringLiteral(_) | TokenKind::ImagNumber { .. } => {}
                other => assert_eq!(text, other.spelling()),
            }
        }
    }

    #[test]
    fn spans_are_exact_byte_offsets() {
        //            0123456789
        let toks = lex("x = 1 + 2");
        let spans: Vec<(u32, u32)> = toks.iter().map(|t| (t.span.start, t.span.end)).collect();
        assert_eq!(spans, vec![(0, 1), (2, 3), (4, 5), (6, 7), (8, 9), (9, 9)]);
    }

    // ── operators & delimiters ──────────────────────────────────────────────

    #[test]
    fn single_char_operators() {
        assert_eq!(
            kinds("= + - * / ^ \\ ~ | : < > ( ) [ ] ; , ."),
            vec![
                TokenKind::Eq,
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Times,
                TokenKind::Div,
                TokenKind::Caret,
                TokenKind::Backslash,
                TokenKind::Tilde,
                TokenKind::Pipe,
                TokenKind::Colon,
                TokenKind::Lt,
                TokenKind::Gt,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::LBracket,
                TokenKind::RBracket,
                TokenKind::Semi,
                TokenKind::Comma,
                TokenKind::Dot,
            ]
        );
    }

    #[test]
    fn two_char_operators_beat_their_prefixes() {
        assert_eq!(kinds(":="), vec![TokenKind::Assign]);
        assert_eq!(kinds("<="), vec![TokenKind::Le]);
        assert_eq!(kinds(">="), vec![TokenKind::Ge]);
        assert_eq!(kinds("<>"), vec![TokenKind::Ne]);
        assert_eq!(kinds("->"), vec![TokenKind::Arrow]);
        assert_eq!(kinds(".."), vec![TokenKind::DotDot]);
    }

    #[test]
    fn split_two_char_operators_stay_split() {
        assert_eq!(kinds(": ="), vec![TokenKind::Colon, TokenKind::Eq]);
        assert_eq!(kinds("< ="), vec![TokenKind::Lt, TokenKind::Eq]);
        assert_eq!(kinds("- >"), vec![TokenKind::Minus, TokenKind::Gt]);
        assert_eq!(kinds(". ."), vec![TokenKind::Dot, TokenKind::Dot]);
    }

    #[test]
    fn element_wise_dot_operators() {
        assert_eq!(
            kinds(".* ./ .\\ .^"),
            vec![
                TokenKind::DotStar,
                TokenKind::DotSlash,
                TokenKind::DotBackslash,
                TokenKind::DotCaret,
            ]
        );
    }

    #[test]
    fn member_accessor_and_dotdot_and_leading_dot_number() {
        assert_eq!(
            kinds("HP.out.h"),
            vec![
                ident("HP"),
                TokenKind::Dot,
                ident("out"),
                TokenKind::Dot,
                ident("h"),
            ]
        );
        // `t = 0 .. 600` — the DYNAMIC time span, with and without spaces.
        assert_eq!(
            kinds("0..600"),
            vec![num(0.0, "0"), TokenKind::DotDot, num(600.0, "600")]
        );
        assert_eq!(kinds(".5"), vec![num(0.5, ".5")]);
        // A dot run before a digit: `..5` is DOTDOT then NUMBER, never `.` `.5`.
        assert_eq!(kinds("..5"), vec![TokenKind::DotDot, num(5.0, "5")]);
    }

    #[test]
    fn relational_operators_in_context() {
        assert_eq!(
            kinds("IF x <= 3 AND y <> 2 THEN"),
            vec![
                TokenKind::If,
                ident("x"),
                TokenKind::Le,
                num(3.0, "3"),
                TokenKind::And,
                ident("y"),
                TokenKind::Ne,
                num(2.0, "2"),
                TokenKind::Then,
            ]
        );
    }

    // ── keywords ────────────────────────────────────────────────────────────

    #[test]
    fn keywords_are_case_insensitive() {
        assert_eq!(kinds("for FOR FoR fOr"), vec![TokenKind::For; 4]);
        assert_eq!(kinds("end END eNd"), vec![TokenKind::End; 3]);
        assert_eq!(
            kinds("function PROCEDURE Module"),
            vec![TokenKind::Function, TokenKind::Procedure, TokenKind::Module]
        );
    }

    #[test]
    fn every_keyword_is_recognised() {
        let cases: &[(&str, TokenKind)] = &[
            ("for", TokenKind::For),
            ("to", TokenKind::To),
            ("while", TokenKind::While),
            ("do", TokenKind::Do),
            ("end", TokenKind::End),
            ("function", TokenKind::Function),
            ("procedure", TokenKind::Procedure),
            ("module", TokenKind::Module),
            ("table", TokenKind::Table),
            ("parametric", TokenKind::Parametric),
            ("plot", TokenKind::Plot),
            ("dynamic", TokenKind::Dynamic),
            ("guess", TokenKind::Guess),
            ("linearize", TokenKind::Linearize),
            ("input", TokenKind::Input),
            ("output", TokenKind::Output),
            ("event", TokenKind::Event),
            ("component", TokenKind::Component),
            ("connect", TokenKind::Connect),
            ("param", TokenKind::Param),
            ("variant", TokenKind::Variant),
            ("require", TokenKind::Require),
            ("if", TokenKind::If),
            ("then", TokenKind::Then),
            ("else", TokenKind::Else),
            ("repeat", TokenKind::Repeat),
            ("until", TokenKind::Until),
            ("call", TokenKind::Call),
            ("symbolic", TokenKind::Symbolic),
            ("and", TokenKind::And),
            ("or", TokenKind::Or),
            ("not", TokenKind::Not),
        ];
        for (word, want) in cases {
            assert_eq!(kinds(word), vec![want.clone()], "lexing `{word}`");
            assert_eq!(
                kinds(&word.to_uppercase()),
                vec![want.clone()],
                "lexing `{}`",
                word.to_uppercase()
            );
        }
    }

    #[test]
    fn keywords_are_never_prefix_matched() {
        // The classic ANTLR trap: `format` must not lex as FOR + `mat`.
        for word in [
            "format", "endpoint", "iffy", "table_x", "notation", "orifice", "andrew", "input2",
            "callback", "doit", "toe",
        ] {
            assert_eq!(kinds(word), vec![ident(word)], "`{word}` must be an IDENT");
        }
    }

    #[test]
    fn trailing_sigils_belong_to_the_identifier_and_kill_keywordness() {
        assert_eq!(kinds("R$"), vec![ident("R$")]);
        assert_eq!(kinds("sigma#"), vec![ident("sigma#")]);
        assert_eq!(kinds("end$"), vec![ident("end$")]);
        assert_eq!(kinds("IF#"), vec![ident("IF#")]);
        // At most one sigil, and only at the very end.
        assert_eq!(kinds("R$ x"), vec![ident("R$"), ident("x")]);
        assert_eq!(kinds("a#b"), vec![ident("a#"), ident("b")]);
    }

    #[test]
    fn second_sigil_is_an_error_not_a_silent_skip() {
        // `R$$` lexes IDENT(R$) then hits a bare `$`, which matches no rule.
        let e = err("R$$ = 1");
        assert!(matches!(e, FreesError::Parse { .. }));
        assert_eq!(e.span(), Some(Span::new(2, 3)));
    }

    #[test]
    fn identifier_case_and_underscores_are_preserved() {
        assert_eq!(kinds("T_in_2"), vec![ident("T_in_2")]);
        assert_eq!(kinds("mDot"), vec![ident("mDot")]);
        // An identifier cannot *start* with a digit or an underscore.
        assert_eq!(
            kinds("x2 y_"),
            vec![ident("x2"), ident("y_")],
            "digits and underscores are fine after the first letter"
        );
    }

    // ── STATE TABLE ─────────────────────────────────────────────────────────

    #[test]
    fn state_table_is_one_token() {
        let toks = lex("STATE TABLE WaterCircuit(Pw1)");
        assert_eq!(toks[0].kind, TokenKind::StateTable);
        assert_eq!(toks[0].span, Span::new(0, 11));
        assert_eq!(
            toks[0].span.slice("STATE TABLE WaterCircuit(Pw1)"),
            "STATE TABLE"
        );
        assert_eq!(toks[1].kind, ident("WaterCircuit"));
    }

    #[test]
    fn state_table_accepts_any_run_of_spaces_and_tabs_and_any_casing() {
        for src in [
            "state table",
            "STATE\tTABLE",
            "State \t  TaBlE",
            "sTaTe  table",
        ] {
            assert_eq!(kinds(src), vec![TokenKind::StateTable], "lexing {src:?}");
        }
    }

    #[test]
    fn lone_state_stays_an_identifier() {
        assert_eq!(kinds("state"), vec![ident("state")]);
        assert_eq!(
            kinds("state = 1"),
            vec![ident("state"), TokenKind::Eq, num(1.0, "1")]
        );
        assert_eq!(
            kinds("state tab"),
            vec![ident("state"), ident("tab")],
            "the second word must actually be `table`"
        );
        // A line break does not separate the two halves — `[ \t]+` only.
        assert_eq!(
            kinds("state\nTABLE"),
            vec![ident("state"), TokenKind::Newline, TokenKind::Table]
        );
        // A sigil disqualifies it, as does a longer first word.
        assert_eq!(
            kinds("state$ table"),
            vec![ident("state$"), TokenKind::Table]
        );
        assert_eq!(
            kinds("states table"),
            vec![ident("states"), TokenKind::Table]
        );
    }

    #[test]
    fn lone_table_is_still_the_table_keyword() {
        assert_eq!(kinds("TABLE cp(T)")[0], TokenKind::Table);
    }

    #[test]
    fn state_table_has_no_word_boundary_check_just_like_antlr() {
        // Documented ANTLR wart: maximal munch takes STATETABLE out of
        // `state tables`, leaving a stray `s`. Faithful to the Java oracle.
        assert_eq!(
            kinds("state tables"),
            vec![TokenKind::StateTable, ident("s")]
        );
    }

    // ── numbers ─────────────────────────────────────────────────────────────

    #[test]
    fn plain_and_fractional_numbers() {
        assert_eq!(kinds("0"), vec![num(0.0, "0")]);
        assert_eq!(kinds("42"), vec![num(42.0, "42")]);
        assert_eq!(kinds("12.03125"), vec![num(12.03125, "12.03125")]);
        assert_eq!(kinds("0.001"), vec![num(0.001, "0.001")]);
        assert_eq!(kinds("100.0"), vec![num(100.0, "100.0")]);
    }

    #[test]
    fn leading_dot_numbers() {
        assert_eq!(kinds(".5"), vec![num(0.5, ".5")]);
        assert_eq!(kinds(".25e2"), vec![num(25.0, ".25e2")]);
        assert_eq!(
            kinds("x = .5"),
            vec![ident("x"), TokenKind::Eq, num(0.5, ".5")]
        );
    }

    #[test]
    fn exponents_in_both_cases_and_both_signs() {
        assert_eq!(kinds("1e3"), vec![num(1000.0, "1e3")]);
        assert_eq!(kinds("1E3"), vec![num(1000.0, "1E3")]);
        assert_eq!(kinds("1e-3"), vec![num(0.001, "1e-3")]);
        assert_eq!(kinds("2.5E+4"), vec![num(25000.0, "2.5E+4")]);
        assert_eq!(kinds("6.022e23"), vec![num(6.022e23, "6.022e23")]);
    }

    #[test]
    fn trailing_dot_is_not_part_of_the_number() {
        // `('.' DIGIT+)?` needs digits after the dot.
        assert_eq!(kinds("1."), vec![num(1.0, "1"), TokenKind::Dot]);
        assert_eq!(
            kinds("1.e5"),
            vec![num(1.0, "1"), TokenKind::Dot, ident("e5")]
        );
        // ... which is what makes member access on a numeric-looking name work.
        assert_eq!(
            kinds("1.x"),
            vec![num(1.0, "1"), TokenKind::Dot, ident("x")]
        );
    }

    #[test]
    fn incomplete_exponents_backtrack() {
        assert_eq!(kinds("3e"), vec![num(3.0, "3"), ident("e")]);
        assert_eq!(
            kinds("3e+"),
            vec![num(3.0, "3"), ident("e"), TokenKind::Plus]
        );
        assert_eq!(
            kinds("3e-x"),
            vec![num(3.0, "3"), ident("e"), TokenKind::Minus, ident("x")]
        );
    }

    #[test]
    fn a_minus_sign_is_a_separate_token() {
        assert_eq!(
            kinds("-5"),
            vec![TokenKind::Minus, num(5.0, "5")],
            "signedNumber is a parser rule, not a lexer rule"
        );
    }

    #[test]
    fn number_spans_cover_the_whole_literal() {
        let src = "y = 2.5E+4";
        let toks = lex(src);
        assert_eq!(toks[2].span, Span::new(4, 10));
        assert_eq!(toks[2].span.slice(src), "2.5E+4");
    }

    // ── imaginary numbers ───────────────────────────────────────────────────

    #[test]
    fn imaginary_suffixes() {
        assert_eq!(kinds("2i"), vec![imag(2.0, "2i")]);
        assert_eq!(kinds("2I"), vec![imag(2.0, "2I")]);
        assert_eq!(kinds("2j"), vec![imag(2.0, "2j")]);
        assert_eq!(kinds("2J"), vec![imag(2.0, "2J")]);
        assert_eq!(kinds("3.5j"), vec![imag(3.5, "3.5j")]);
        assert_eq!(kinds(".5i"), vec![imag(0.5, ".5i")]);
        assert_eq!(kinds("1e3i"), vec![imag(1000.0, "1e3i")]);
        assert_eq!(kinds("2.5E-2J"), vec![imag(0.025, "2.5E-2J")]);
    }

    #[test]
    fn imaginary_value_excludes_the_suffix_but_text_keeps_it() {
        match &kinds("4.25i")[0] {
            TokenKind::ImagNumber { value, text } => {
                assert_eq!(*value, 4.25);
                assert_eq!(text, "4.25i");
            }
            other => panic!("expected an imaginary number, got {other:?}"),
        }
    }

    #[test]
    fn a_letter_after_the_imaginary_suffix_starts_a_new_token() {
        assert_eq!(kinds("2ix"), vec![imag(2.0, "2i"), ident("x")]);
        // `e` is not an imaginary suffix, so `2e` is NUMBER + IDENT.
        assert_eq!(kinds("2e"), vec![num(2.0, "2"), ident("e")]);
        // Nor is `2k`.
        assert_eq!(kinds("2k"), vec![num(2.0, "2"), ident("k")]);
    }

    #[test]
    fn imaginary_in_an_expression() {
        assert_eq!(
            kinds("z = 3 + 4i"),
            vec![
                ident("z"),
                TokenKind::Eq,
                num(3.0, "3"),
                TokenKind::Plus,
                imag(4.0, "4i"),
            ]
        );
    }

    // ── string literals ─────────────────────────────────────────────────────

    #[test]
    fn string_literals_strip_their_quotes() {
        assert_eq!(kinds("'R134a'"), vec![string("R134a")]);
        assert_eq!(kinds("''"), vec![string("")]);
        assert_eq!(kinds("'Speed [m/s]'"), vec![string("Speed [m/s]")]);
        assert_eq!(
            kinds("R$ = 'Water'"),
            vec![ident("R$"), TokenKind::Eq, string("Water")]
        );
    }

    #[test]
    fn string_spans_include_the_quotes() {
        let src = "name = 'x y'";
        let toks = lex(src);
        assert_eq!(toks[2].kind, string("x y"));
        assert_eq!(toks[2].span, Span::new(7, 12));
        assert_eq!(toks[2].span.slice(src), "'x y'");
    }

    #[test]
    fn a_double_quote_inside_a_string_is_content_not_a_comment() {
        assert_eq!(kinds("'say \"hi\"'"), vec![string("say \"hi\"")]);
    }

    #[test]
    fn braces_and_slashes_inside_a_string_are_content() {
        assert_eq!(
            kinds("'{not a comment} // nor this'"),
            vec![string("{not a comment} // nor this")]
        );
    }

    #[test]
    fn two_strings_on_one_line() {
        assert_eq!(
            kinds("PLOT 'a', 'b'"),
            vec![TokenKind::Plot, string("a"), TokenKind::Comma, string("b")]
        );
    }

    // ── transpose vs string (the documented tradeoff) ───────────────────────

    #[test]
    fn a_quote_with_no_closing_quote_is_a_transpose() {
        assert_eq!(kinds("A'"), vec![ident("A"), TokenKind::Transpose]);
        assert_eq!(
            kinds("B = A'"),
            vec![ident("B"), TokenKind::Eq, ident("A"), TokenKind::Transpose]
        );
        assert_eq!(
            kinds("y = (A*x)'"),
            vec![
                ident("y"),
                TokenKind::Eq,
                TokenKind::LParen,
                ident("A"),
                TokenKind::Times,
                ident("x"),
                TokenKind::RParen,
                TokenKind::Transpose,
            ]
        );
        assert_eq!(
            kinds("C = A' * B"),
            vec![
                ident("C"),
                TokenKind::Eq,
                ident("A"),
                TokenKind::Transpose,
                TokenKind::Times,
                ident("B"),
            ]
        );
    }

    #[test]
    fn transpose_spans_one_byte() {
        let src = "A'";
        let toks = lex(src);
        assert_eq!(toks[1].span, Span::new(1, 2));
        assert_eq!(toks[1].span.slice(src), "'");
    }

    #[test]
    fn two_transposes_on_one_line_lex_as_a_string_like_antlr() {
        // THE TRADEOFF, spelled out: ANTLR's maximal munch prefers the longer
        // STRING_LITERAL match, so the text between the two quotes becomes a
        // string. This is the Java engine's behaviour and the port reproduces
        // it rather than silently diverging from the oracle.
        assert_eq!(kinds("A' + B'"), vec![ident("A"), string(" + B")]);
        // The same wart across lines — `~'\''` matches newlines too.
        assert_eq!(kinds("A'\nB'"), vec![ident("A"), string("\nB")]);
    }

    #[test]
    fn a_transpose_followed_by_a_real_string_still_misbinds_faithfully() {
        // `x = A' ; s$ = 'w'` — the first quote pairs with the opening quote of
        // the later literal, exactly as ANTLR does it.
        let toks = kinds("x = A'; s$ = 'w'");
        assert_eq!(toks[3], string("; s$ = "));
    }

    // ── comments ────────────────────────────────────────────────────────────

    #[test]
    fn brace_comments_are_skipped() {
        assert_eq!(
            kinds("a { this is a comment } = 1"),
            vec![ident("a"), TokenKind::Eq, num(1.0, "1")]
        );
        assert_eq!(kinds("{only a comment}"), vec![]);
        assert_eq!(
            kinds("{c1} x {c2} = {c3} 2"),
            vec![ident("x"), TokenKind::Eq, num(2.0, "2")]
        );
    }

    #[test]
    fn brace_comments_are_non_greedy_and_do_not_nest() {
        // `.*?` stops at the FIRST '}'. The rest is ordinary input.
        assert_eq!(
            kinds("{a {b} c"),
            vec![ident("c")],
            "the comment ends at the first `}}`"
        );
    }

    #[test]
    fn brace_comments_may_span_lines_and_swallow_the_newline() {
        assert_eq!(
            kinds("a {line one\nline two} b"),
            vec![ident("a"), ident("b")],
            "a multi-line comment removes the line break with it"
        );
    }

    #[test]
    fn quote_comments_are_skipped() {
        assert_eq!(
            kinds("a \"a note\" = 1"),
            vec![ident("a"), TokenKind::Eq, num(1.0, "1")]
        );
        assert_eq!(kinds("\"just a note\""), vec![]);
        // An apostrophe inside a quote comment does not open a string.
        assert_eq!(
            kinds("x \"it's fine\" = 2"),
            vec![ident("x"), TokenKind::Eq, num(2.0, "2")]
        );
    }

    #[test]
    fn line_comments_run_to_end_of_line_and_keep_the_newline() {
        assert_eq!(
            kinds("a = 1 // trailing note\nb = 2"),
            vec![
                ident("a"),
                TokenKind::Eq,
                num(1.0, "1"),
                TokenKind::Newline,
                ident("b"),
                TokenKind::Eq,
                num(2.0, "2"),
            ]
        );
        assert_eq!(kinds("// whole line"), vec![]);
        assert_eq!(kinds("// crlf\r\nx"), vec![TokenKind::Newline, ident("x")]);
    }

    #[test]
    fn a_single_slash_is_division_not_a_comment() {
        assert_eq!(kinds("a / b"), vec![ident("a"), TokenKind::Div, ident("b")]);
        assert_eq!(
            kinds("a ./ b"),
            vec![ident("a"), TokenKind::DotSlash, ident("b")]
        );
    }

    #[test]
    fn adjacent_comments_are_all_skipped() {
        assert_eq!(
            kinds("{a}\"b\"{c} x"),
            vec![ident("x")],
            "skip_trivia loops over back-to-back comments"
        );
    }

    #[test]
    fn unterminated_comments_are_errors_with_a_span_on_the_opener() {
        let e = err("x = 1 { never closed");
        assert_eq!(e.span(), Some(Span::new(6, 7)));
        match &e {
            FreesError::Parse { message, .. } => {
                assert!(message.contains("unterminated"), "message was {message:?}");
                assert!(message.contains('}'), "message was {message:?}");
            }
            other => panic!("expected a parse error, got {other:?}"),
        }

        let e = err("x = 1 \" never closed");
        assert_eq!(e.span(), Some(Span::new(6, 7)));
    }

    // ── newlines ────────────────────────────────────────────────────────────

    #[test]
    fn a_run_of_newlines_collapses_to_one_token() {
        let src = "a\n\n\nb";
        let toks = lex(src);
        assert_eq!(
            toks.iter().map(|t| t.kind.clone()).collect::<Vec<_>>(),
            vec![ident("a"), TokenKind::Newline, ident("b"), TokenKind::Eof]
        );
        assert_eq!(
            toks[1].span,
            Span::new(1, 4),
            "the span covers the whole run"
        );
    }

    #[test]
    fn crlf_runs_collapse_too() {
        let toks = lex("a\r\n\r\nb");
        assert_eq!(toks[1].kind, TokenKind::Newline);
        assert_eq!(toks[1].span, Span::new(1, 5));
        assert_eq!(toks[2].kind, ident("b"));
        // Mixed endings inside one run.
        let toks = lex("a\n\r\n\nb");
        assert_eq!(toks[1].span, Span::new(1, 5));
    }

    #[test]
    fn whitespace_between_line_breaks_splits_the_run() {
        // NEWLINE is ('\r'? '\n')+ — a space is not part of it, so a blank line
        // containing a space yields two NEWLINE tokens.
        assert_eq!(
            kinds("a\n \nb"),
            vec![
                ident("a"),
                TokenKind::Newline,
                TokenKind::Newline,
                ident("b")
            ]
        );
    }

    #[test]
    fn a_trailing_newline_still_emits_a_token() {
        assert_eq!(kinds("a\n"), vec![ident("a"), TokenKind::Newline]);
        assert_eq!(kinds("\na"), vec![TokenKind::Newline, ident("a")]);
    }

    #[test]
    fn a_lone_carriage_return_is_an_error() {
        let e = err("a\rb");
        assert_eq!(e.span(), Some(Span::new(1, 2)));
        match &e {
            FreesError::Parse { message, .. } => {
                assert!(
                    message.contains("carriage return"),
                    "message was {message:?}"
                )
            }
            other => panic!("expected a parse error, got {other:?}"),
        }
    }

    #[test]
    fn semicolons_are_separators_not_newlines() {
        assert_eq!(
            kinds("a = 1; b = 2"),
            vec![
                ident("a"),
                TokenKind::Eq,
                num(1.0, "1"),
                TokenKind::Semi,
                ident("b"),
                TokenKind::Eq,
                num(2.0, "2"),
            ]
        );
        assert!(TokenKind::Semi.is_separator());
        assert!(TokenKind::Newline.is_separator());
    }

    // ── errors ──────────────────────────────────────────────────────────────

    #[test]
    fn an_unexpected_character_is_a_parse_error_with_a_span() {
        for (src, offset, ch) in [
            ("a @ b", 2, '@'),
            ("x = 1 ? 2", 6, '?'),
            ("a & b", 2, '&'),
            ("a % b", 2, '%'),
            ("_x = 1", 0, '_'),
            ("a } b", 2, '}'),
        ] {
            let e = err(src);
            assert_eq!(e.span(), Some(Span::new(offset, offset + 1)), "for {src:?}");
            match &e {
                FreesError::Parse { message, .. } => assert!(
                    message.contains(ch),
                    "message {message:?} should quote {ch:?}"
                ),
                other => panic!("expected a parse error, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_non_ascii_character_reports_a_whole_char_span() {
        let src = "a = π";
        let e = err(src);
        assert_eq!(e.span(), Some(Span::new(4, 6)), "π is two bytes");
        assert_eq!(e.span().unwrap().slice(src), "π");
        assert_eq!(e.span().unwrap().line_col(src), (1, 5));
    }

    #[test]
    fn errors_carry_a_usable_line_and_column() {
        let src = "a = 1\nb = 2\nc = @";
        let e = err(src);
        assert_eq!(e.span().unwrap().line_col(src), (3, 5));
    }

    #[test]
    fn non_ascii_inside_comments_and_strings_is_fine() {
        assert_eq!(kinds("x { π ≈ 3 } = 1")[0], ident("x"));
        assert_eq!(kinds("'π'"), vec![string("π")]);
        assert_eq!(kinds("\"note: π\""), vec![]);
    }

    #[test]
    fn a_byte_order_mark_is_tolerated() {
        assert_eq!(
            kinds("\u{feff}x = 1"),
            vec![ident("x"), TokenKind::Eq, num(1.0, "1")]
        );
    }

    // ── realistic documents ─────────────────────────────────────────────────

    #[test]
    fn a_realistic_equation_document() {
        let src = "\
{ Simple cycle }
T_in = 300 [K]
P$ = 'R134a'
h_2 = Enthalpy(P$, T=T_in, x=1)   // property call
";
        assert_eq!(
            kinds(src),
            vec![
                TokenKind::Newline,
                ident("T_in"),
                TokenKind::Eq,
                num(300.0, "300"),
                TokenKind::LBracket,
                ident("K"),
                TokenKind::RBracket,
                TokenKind::Newline,
                ident("P$"),
                TokenKind::Eq,
                string("R134a"),
                TokenKind::Newline,
                ident("h_2"),
                TokenKind::Eq,
                ident("Enthalpy"),
                TokenKind::LParen,
                ident("P$"),
                TokenKind::Comma,
                ident("T"),
                TokenKind::Eq,
                ident("T_in"),
                TokenKind::Comma,
                ident("x"),
                TokenKind::Eq,
                num(1.0, "1"),
                TokenKind::RParen,
                TokenKind::Newline,
            ]
        );
    }

    #[test]
    fn a_for_block_lexes_end_to_end() {
        assert_eq!(
            kinds("FOR i = 1 TO N\n  x[i] = i^2\nEND"),
            vec![
                TokenKind::For,
                ident("i"),
                TokenKind::Eq,
                num(1.0, "1"),
                TokenKind::To,
                ident("N"),
                TokenKind::Newline,
                ident("x"),
                TokenKind::LBracket,
                ident("i"),
                TokenKind::RBracket,
                TokenKind::Eq,
                ident("i"),
                TokenKind::Caret,
                num(2.0, "2"),
                TokenKind::Newline,
                TokenKind::End,
            ]
        );
    }

    #[test]
    fn a_dynamic_header_with_a_range_and_units() {
        assert_eq!(
            kinds("DYNAMIC cooling (method = ode45, t = 0 .. 600 [s], rtol = 1e-6)"),
            vec![
                TokenKind::Dynamic,
                ident("cooling"),
                TokenKind::LParen,
                ident("method"),
                TokenKind::Eq,
                ident("ode45"),
                TokenKind::Comma,
                ident("t"),
                TokenKind::Eq,
                num(0.0, "0"),
                TokenKind::DotDot,
                num(600.0, "600"),
                TokenKind::LBracket,
                ident("s"),
                TokenKind::RBracket,
                TokenKind::Comma,
                ident("rtol"),
                TokenKind::Eq,
                num(1e-6, "1e-6"),
                TokenKind::RParen,
            ]
        );
    }

    #[test]
    fn an_event_line_with_pipe_and_arrow() {
        assert_eq!(
            kinds("EVENT cool: T = T_inf | falling -> stop"),
            vec![
                TokenKind::Event,
                ident("cool"),
                TokenKind::Colon,
                ident("T"),
                TokenKind::Eq,
                ident("T_inf"),
                TokenKind::Pipe,
                ident("falling"),
                TokenKind::Arrow,
                ident("stop"),
            ]
        );
    }

    #[test]
    fn a_multi_assign_with_a_discarded_output() {
        assert_eq!(
            kinds("[~, ~, V] = svd(A)"),
            vec![
                TokenKind::LBracket,
                TokenKind::Tilde,
                TokenKind::Comma,
                TokenKind::Tilde,
                TokenKind::Comma,
                ident("V"),
                TokenKind::RBracket,
                TokenKind::Eq,
                ident("svd"),
                TokenKind::LParen,
                ident("A"),
                TokenKind::RParen,
            ]
        );
    }

    #[test]
    fn a_procedural_body_with_assignment_and_relational_ops() {
        assert_eq!(
            kinds("IF x >= 0 THEN\n  y := x .* 2\nELSE\n  y := -x\nEND"),
            vec![
                TokenKind::If,
                ident("x"),
                TokenKind::Ge,
                num(0.0, "0"),
                TokenKind::Then,
                TokenKind::Newline,
                ident("y"),
                TokenKind::Assign,
                ident("x"),
                TokenKind::DotStar,
                num(2.0, "2"),
                TokenKind::Newline,
                TokenKind::Else,
                TokenKind::Newline,
                ident("y"),
                TokenKind::Assign,
                TokenKind::Minus,
                ident("x"),
                TokenKind::Newline,
                TokenKind::End,
            ]
        );
    }

    #[test]
    fn a_guess_directive_with_bounds() {
        assert_eq!(
            kinds("GUESS x = 2 [0, 10]"),
            vec![
                TokenKind::Guess,
                ident("x"),
                TokenKind::Eq,
                num(2.0, "2"),
                TokenKind::LBracket,
                num(0.0, "0"),
                TokenKind::Comma,
                num(10.0, "10"),
                TokenKind::RBracket,
            ]
        );
    }

    #[test]
    fn a_unit_annotation_lexes_as_ordinary_tokens() {
        assert_eq!(
            kinds("[kJ/kg-K]"),
            vec![
                TokenKind::LBracket,
                ident("kJ"),
                TokenKind::Div,
                ident("kg"),
                TokenKind::Minus,
                ident("K"),
                TokenKind::RBracket,
            ]
        );
        assert_eq!(
            kinds("[m^2]"),
            vec![
                TokenKind::LBracket,
                ident("m"),
                TokenKind::Caret,
                num(2.0, "2"),
                TokenKind::RBracket,
            ]
        );
    }

    #[test]
    fn a_state_table_block() {
        assert_eq!(
            kinds("STATE TABLE Circuit(Pw1, Tw1)\n  FLUID = Water\nEND"),
            vec![
                TokenKind::StateTable,
                ident("Circuit"),
                TokenKind::LParen,
                ident("Pw1"),
                TokenKind::Comma,
                ident("Tw1"),
                TokenKind::RParen,
                TokenKind::Newline,
                ident("FLUID"),
                TokenKind::Eq,
                ident("Water"),
                TokenKind::Newline,
                TokenKind::End,
            ]
        );
    }

    #[test]
    fn matrix_and_backslash_solve() {
        assert_eq!(
            kinds("x = A \\ b"),
            vec![
                ident("x"),
                TokenKind::Eq,
                ident("A"),
                TokenKind::Backslash,
                ident("b"),
            ]
        );
        assert_eq!(
            kinds("M = [1, 2; 3, 4]"),
            vec![
                ident("M"),
                TokenKind::Eq,
                TokenKind::LBracket,
                num(1.0, "1"),
                TokenKind::Comma,
                num(2.0, "2"),
                TokenKind::Semi,
                num(3.0, "3"),
                TokenKind::Comma,
                num(4.0, "4"),
                TokenKind::RBracket,
            ]
        );
    }

    #[test]
    fn array_slices_use_colons() {
        assert_eq!(
            kinds("y = speed[1:N]"),
            vec![
                ident("y"),
                TokenKind::Eq,
                ident("speed"),
                TokenKind::LBracket,
                num(1.0, "1"),
                TokenKind::Colon,
                ident("N"),
                TokenKind::RBracket,
            ]
        );
    }

    #[test]
    fn a_range_assign_with_a_spacing_flag() {
        assert_eq!(
            kinds("freq = 1:5:1000 | Log"),
            vec![
                ident("freq"),
                TokenKind::Eq,
                num(1.0, "1"),
                TokenKind::Colon,
                num(5.0, "5"),
                TokenKind::Colon,
                num(1000.0, "1000"),
                TokenKind::Pipe,
                ident("Log"),
            ]
        );
    }

    #[test]
    fn describe_and_spelling_survive_a_round_trip() {
        // Sanity check that the token payloads the parser will quote are sane.
        let toks = kinds("alpha = 'txt' + 2.5i");
        assert_eq!(toks[0].describe(), "identifier `alpha`");
        assert_eq!(toks[1].describe(), "`=`");
        assert_eq!(toks[2].describe(), "string `'txt'`");
        assert_eq!(toks[4].describe(), "imaginary number `2.5i`");
    }
}
