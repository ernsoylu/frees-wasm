//! Token definitions — the contract between the lexer and the parser.
//!
//! Derived directly from the lexer section of
//! `../frEES/backend/core/src/main/antlr/Frees.g4` (lines 499–632). The token
//! set is exhaustive; ordering subtleties from the ANTLR grammar are recorded
//! on the variants they affect.

use crate::diag::Span;

/// A lexed token: kind plus its byte span in the source document.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Token {
        Token { kind, span }
    }
}

/// Every token the frees grammar produces.
///
/// # Lexing rules that must be honoured
///
/// * **Keywords are case-insensitive** (`[fF][oO][rR]` etc.). Scan a full
///   identifier first, then classify — otherwise `format` lexes as `FOR` + `mat`.
/// * **A trailing `$` or `#` is part of the identifier** (`R$`, `sigma#`), so
///   `end$` is an [`TokenKind::Ident`], never [`TokenKind::End`].
/// * **`STATE TABLE` is one token** separated by spaces/tabs, matched before
///   `TABLE`. A lone `state` stays an identifier.
/// * **Longest match around `.`**: `..` → [`DotDot`](TokenKind::DotDot),
///   `.*` `./` `.\` `.^` → the element-wise operators, `.5` → a
///   [`Number`](TokenKind::Number), and a lone `.` → [`Dot`](TokenKind::Dot).
/// * **`:=` beats `:`**, and `<=` `>=` `<>` beat `<` `>`.
/// * **`NEWLINE` collapses a run** of `('\r'? '\n')+` into a single token.
/// * **Three comment forms are skipped**: `{ … }`, `" … "`, and `// …` to
///   end of line. Note that `"` delimits a *comment*, not a string — string
///   literals use single quotes.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // ── Literals & identifiers ──────────────────────────────────────────────
    /// Numeric literal. `text` is the original spelling, kept for diagnostics.
    Number {
        value: f64,
        text: String,
    },
    /// Imaginary literal — a number with an `i`/`I`/`j`/`J` suffix.
    ImagNumber {
        value: f64,
        text: String,
    },
    /// `'…'` — contents only, quotes stripped.
    StringLiteral(String),
    /// Identifier in its **original case**, including any trailing `$` or `#`.
    /// AST constructors lowercase it; diagnostics want the user's spelling.
    Ident(String),

    // ── Operators ───────────────────────────────────────────────────────────
    /// `=`
    Eq,
    /// `:=`
    Assign,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Times,
    /// `/`
    Div,
    /// `^`
    Caret,
    /// `\`
    Backslash,
    /// `.*`
    DotStar,
    /// `./`
    DotSlash,
    /// `.\`
    DotBackslash,
    /// `.^`
    DotCaret,
    /// `'` — postfix transpose.
    Transpose,
    /// `~` — discarded output slot in a destructuring call.
    Tilde,
    /// `|`
    Pipe,
    /// `..` — retained only for `DYNAMIC` time spans (`t = 0 .. 600`).
    DotDot,
    /// `.` — member accessor (`in.P`, `HP.out.h`).
    Dot,
    /// `:`
    Colon,
    /// `->`
    Arrow,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `<>`
    Ne,

    // ── Delimiters ──────────────────────────────────────────────────────────
    LParen,
    RParen,
    LBracket,
    RBracket,
    /// `;` — a statement separator, and the row separator in matrix literals.
    Semi,
    Comma,

    // ── Keywords (case-insensitive) ─────────────────────────────────────────
    For,
    To,
    While,
    Do,
    End,
    Function,
    Procedure,
    Module,
    /// `STATE TABLE` — two words, one token.
    StateTable,
    Table,
    Parametric,
    Plot,
    Dynamic,
    Guess,
    Linearize,
    Input,
    Output,
    Event,
    Component,
    Connect,
    Param,
    Variant,
    Require,
    If,
    Then,
    Else,
    Repeat,
    Until,
    Call,
    Symbolic,
    And,
    Or,
    Not,

    // ── Trivia & terminator ─────────────────────────────────────────────────
    /// One or more physical line breaks, collapsed.
    Newline,
    /// End of input.
    Eof,
}

impl TokenKind {
    /// Classify an already-scanned identifier, honouring case-insensitivity.
    ///
    /// Returns [`TokenKind::Ident`] when the word is not a keyword. A word
    /// carrying a trailing `$` or `#` is never a keyword.
    ///
    /// `STATE TABLE` is not handled here — it spans whitespace and must be
    /// recognised by the lexer's scanner.
    pub fn keyword_or_ident(word: &str) -> TokenKind {
        if word.ends_with('$') || word.ends_with('#') {
            return TokenKind::Ident(word.to_string());
        }
        match word.to_ascii_lowercase().as_str() {
            "for" => TokenKind::For,
            "to" => TokenKind::To,
            "while" => TokenKind::While,
            "do" => TokenKind::Do,
            "end" => TokenKind::End,
            "function" => TokenKind::Function,
            "procedure" => TokenKind::Procedure,
            "module" => TokenKind::Module,
            "table" => TokenKind::Table,
            "parametric" => TokenKind::Parametric,
            "plot" => TokenKind::Plot,
            "dynamic" => TokenKind::Dynamic,
            "guess" => TokenKind::Guess,
            "linearize" => TokenKind::Linearize,
            "input" => TokenKind::Input,
            "output" => TokenKind::Output,
            "event" => TokenKind::Event,
            "component" => TokenKind::Component,
            "connect" => TokenKind::Connect,
            "param" => TokenKind::Param,
            "variant" => TokenKind::Variant,
            "require" => TokenKind::Require,
            "if" => TokenKind::If,
            "then" => TokenKind::Then,
            "else" => TokenKind::Else,
            "repeat" => TokenKind::Repeat,
            "until" => TokenKind::Until,
            "call" => TokenKind::Call,
            "symbolic" => TokenKind::Symbolic,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            _ => TokenKind::Ident(word.to_string()),
        }
    }

    /// True when this token ends a statement (`;` or a newline run).
    pub fn is_separator(&self) -> bool {
        matches!(self, TokenKind::Semi | TokenKind::Newline)
    }

    /// Human-readable name used in "expected X, found Y" diagnostics.
    pub fn describe(&self) -> String {
        match self {
            TokenKind::Number { text, .. } => format!("number `{text}`"),
            TokenKind::ImagNumber { text, .. } => format!("imaginary number `{text}`"),
            TokenKind::StringLiteral(s) => format!("string `'{s}'`"),
            TokenKind::Ident(name) => format!("identifier `{name}`"),
            TokenKind::Newline => "end of line".to_string(),
            TokenKind::Eof => "end of input".to_string(),
            other => format!("`{}`", other.spelling()),
        }
    }

    /// Canonical source spelling for fixed tokens.
    pub fn spelling(&self) -> &'static str {
        match self {
            TokenKind::Eq => "=",
            TokenKind::Assign => ":=",
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Times => "*",
            TokenKind::Div => "/",
            TokenKind::Caret => "^",
            TokenKind::Backslash => "\\",
            TokenKind::DotStar => ".*",
            TokenKind::DotSlash => "./",
            TokenKind::DotBackslash => ".\\",
            TokenKind::DotCaret => ".^",
            TokenKind::Transpose => "'",
            TokenKind::Tilde => "~",
            TokenKind::Pipe => "|",
            TokenKind::DotDot => "..",
            TokenKind::Dot => ".",
            TokenKind::Colon => ":",
            TokenKind::Arrow => "->",
            TokenKind::Lt => "<",
            TokenKind::Gt => ">",
            TokenKind::Le => "<=",
            TokenKind::Ge => ">=",
            TokenKind::Ne => "<>",
            TokenKind::LParen => "(",
            TokenKind::RParen => ")",
            TokenKind::LBracket => "[",
            TokenKind::RBracket => "]",
            TokenKind::Semi => ";",
            TokenKind::Comma => ",",
            TokenKind::For => "FOR",
            TokenKind::To => "TO",
            TokenKind::While => "WHILE",
            TokenKind::Do => "DO",
            TokenKind::End => "END",
            TokenKind::Function => "FUNCTION",
            TokenKind::Procedure => "PROCEDURE",
            TokenKind::Module => "MODULE",
            TokenKind::StateTable => "STATE TABLE",
            TokenKind::Table => "TABLE",
            TokenKind::Parametric => "PARAMETRIC",
            TokenKind::Plot => "PLOT",
            TokenKind::Dynamic => "DYNAMIC",
            TokenKind::Guess => "GUESS",
            TokenKind::Linearize => "LINEARIZE",
            TokenKind::Input => "INPUT",
            TokenKind::Output => "OUTPUT",
            TokenKind::Event => "EVENT",
            TokenKind::Component => "COMPONENT",
            TokenKind::Connect => "CONNECT",
            TokenKind::Param => "PARAM",
            TokenKind::Variant => "VARIANT",
            TokenKind::Require => "REQUIRE",
            TokenKind::If => "IF",
            TokenKind::Then => "THEN",
            TokenKind::Else => "ELSE",
            TokenKind::Repeat => "REPEAT",
            TokenKind::Until => "UNTIL",
            TokenKind::Call => "CALL",
            TokenKind::Symbolic => "SYMBOLIC",
            TokenKind::And => "AND",
            TokenKind::Or => "OR",
            TokenKind::Not => "NOT",
            TokenKind::Newline => "\\n",
            TokenKind::Eof => "<eof>",
            TokenKind::Number { .. } => "<number>",
            TokenKind::ImagNumber { .. } => "<imaginary>",
            TokenKind::StringLiteral(_) => "<string>",
            TokenKind::Ident(_) => "<identifier>",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_are_case_insensitive() {
        assert_eq!(TokenKind::keyword_or_ident("END"), TokenKind::End);
        assert_eq!(TokenKind::keyword_or_ident("end"), TokenKind::End);
        assert_eq!(TokenKind::keyword_or_ident("EnD"), TokenKind::End);
    }

    #[test]
    fn keyword_prefixes_stay_identifiers() {
        // `format` starts with `for`, `endpoint` with `end`, `iffy` with `if`.
        for word in ["format", "endpoint", "iffy", "table_x", "notation"] {
            assert!(
                matches!(TokenKind::keyword_or_ident(word), TokenKind::Ident(_)),
                "{word} should lex as an identifier"
            );
        }
    }

    #[test]
    fn sigil_suffixed_words_are_never_keywords() {
        assert_eq!(
            TokenKind::keyword_or_ident("end$"),
            TokenKind::Ident("end$".into())
        );
        assert_eq!(
            TokenKind::keyword_or_ident("R#"),
            TokenKind::Ident("R#".into())
        );
    }

    #[test]
    fn identifier_case_is_preserved_by_the_lexer() {
        // The AST lowercases; the token stream keeps the user's spelling so
        // diagnostics can quote it verbatim.
        assert_eq!(
            TokenKind::keyword_or_ident("T_in"),
            TokenKind::Ident("T_in".into())
        );
    }
}
