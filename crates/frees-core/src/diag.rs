//! Spans, diagnostics and the crate error type.
//!
//! The parent engine's rule is that diagnostics are **source-mapped** — they
//! quote the user's own text and never leak mangled internal names (see
//! `../frEES/CLAUDE.md`, "Diagnostics — source-mapped (component-named, never
//! mangled scalars)"). Every error therefore carries a [`Span`] into the
//! original document where one is known.

use std::fmt;

/// A half-open byte range `[start, end)` into the source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Span {
        Span {
            start: start as u32,
            end: end as u32,
        }
    }

    /// A zero-width span at `offset`, used for "expected X here" errors.
    pub fn at(offset: usize) -> Span {
        Span::new(offset, offset)
    }

    /// The smallest span covering both operands.
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    pub fn len(self) -> usize {
        (self.end - self.start) as usize
    }

    pub fn is_empty(self) -> bool {
        self.end == self.start
    }

    /// The slice of `source` this span covers, or `""` if out of bounds.
    pub fn slice(self, source: &str) -> &str {
        source
            .get(self.start as usize..self.end as usize)
            .unwrap_or("")
    }

    /// 1-based `(line, column)` of the span start, counted in characters.
    pub fn line_col(self, source: &str) -> (usize, usize) {
        let upto = source.get(..self.start as usize).unwrap_or(source);
        let line = upto.matches('\n').count() + 1;
        let col = match upto.rfind('\n') {
            Some(nl) => upto[nl + 1..].chars().count() + 1,
            None => upto.chars().count() + 1,
        };
        (line, col)
    }
}

/// How serious a diagnostic is.
///
/// **Unit problems are always warnings.** The parent engine's invariant is that
/// dimensional inconsistency never blocks a solve; it is surfaced and the solve
/// proceeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Warning,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Warning => f.write_str("warning"),
            Severity::Error => f.write_str("error"),
        }
    }
}

/// One user-facing message about the document.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Option<Span>,
    /// Optional source line quoted verbatim, for renderers without the document.
    pub source_text: Option<String>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            severity: Severity::Error,
            message: message.into(),
            span: None,
            source_text: None,
        }
    }

    pub fn warning(message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            severity: Severity::Warning,
            message: message.into(),
            span: None,
            source_text: None,
        }
    }

    pub fn with_span(mut self, span: Span) -> Diagnostic {
        self.span = Some(span);
        self
    }

    pub fn with_source_text(mut self, text: impl Into<String>) -> Diagnostic {
        self.source_text = Some(text.into());
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.severity, self.message)
    }
}

/// Errors raised by the engine.
///
/// `Parse` corresponds to the Java `ParseException`, `Solver` to
/// `SolverException`, and `Property` to `PropertyEvaluationException`.
#[derive(Debug, Clone, PartialEq)]
pub enum FreesError {
    /// Lexing or parsing failed. A hard failure — the document is not a program.
    Parse { message: String, span: Option<Span> },
    /// The document parsed but cannot be solved as written (degrees of freedom,
    /// unmatched variables, singular block, no convergence).
    Solver { message: String },
    /// A property call failed for the current state point.
    Property { message: String },
    /// An intrinsic was called with the wrong arity or argument types.
    Evaluation { message: String },
    /// A unit expression could not be resolved.
    UnknownUnit { unit: String },
}

impl FreesError {
    pub fn parse(message: impl Into<String>) -> FreesError {
        FreesError::Parse {
            message: message.into(),
            span: None,
        }
    }

    pub fn parse_at(message: impl Into<String>, span: Span) -> FreesError {
        FreesError::Parse {
            message: message.into(),
            span: Some(span),
        }
    }

    pub fn solver(message: impl Into<String>) -> FreesError {
        FreesError::Solver {
            message: message.into(),
        }
    }

    pub fn evaluation(message: impl Into<String>) -> FreesError {
        FreesError::Evaluation {
            message: message.into(),
        }
    }

    pub fn property(message: impl Into<String>) -> FreesError {
        FreesError::Property {
            message: message.into(),
        }
    }

    pub fn span(&self) -> Option<Span> {
        match self {
            FreesError::Parse { span, .. } => *span,
            _ => None,
        }
    }
}

impl fmt::Display for FreesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FreesError::Parse { message, .. } => write!(f, "parse error: {message}"),
            FreesError::Solver { message } => write!(f, "solver error: {message}"),
            FreesError::Property { message } => write!(f, "property error: {message}"),
            FreesError::Evaluation { message } => write!(f, "evaluation error: {message}"),
            FreesError::UnknownUnit { unit } => write!(f, "unknown unit: {unit}"),
        }
    }
}

impl std::error::Error for FreesError {}

pub type Result<T> = std::result::Result<T, FreesError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_is_one_based() {
        let src = "a = 1\nb = 2\nc = 3";
        assert_eq!(Span::at(0).line_col(src), (1, 1));
        assert_eq!(Span::at(6).line_col(src), (2, 1));
        assert_eq!(Span::at(8).line_col(src), (2, 3));
        assert_eq!(Span::at(12).line_col(src), (3, 1));
    }

    #[test]
    fn spans_slice_and_merge() {
        let src = "alpha + beta";
        let a = Span::new(0, 5);
        let b = Span::new(8, 12);
        assert_eq!(a.slice(src), "alpha");
        assert_eq!(b.slice(src), "beta");
        assert_eq!(a.merge(b).slice(src), "alpha + beta");
    }

    #[test]
    fn out_of_bounds_slice_is_empty_not_panic() {
        assert_eq!(Span::new(10, 20).slice("short"), "");
    }
}
