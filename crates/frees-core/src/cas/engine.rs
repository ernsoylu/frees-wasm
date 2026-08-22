//! The CAS entry points — `CasEngine` and `CasIdentity` without Symja.
//!
//! Port target: `../frEES/backend/core/.../cas/{CasEngine,CasExpressions,
//! CasIdentity}.java`. The Java shape survives (parse a string, run one named
//! operation, hand back an expression plus its LaTeX plus the engine's own
//! printed form); what does not survive is the engine underneath. Every
//! operation routes into [`crate::cas::ops`], which works on an internal
//! rational IR.
//!
//! # What the Java's plumbing was for, and why it is gone
//!
//! * **A thread pool and a three-second timeout.** `CasEngine.evaluate`
//!   submits each call to a daemon executor and cancels it on timeout, because
//!   Symja can run away on a pathological input. wasm is single-threaded and
//!   has nothing to cancel *into*, so the bound has to be structural instead:
//!   [`crate::cas::ops`] caps expression depth, expansion exponent and term
//!   count, and returns [`CasError::TooDeep`] / [`CasError::TooLarge`] rather
//!   than hanging.
//! * **A one-shot warm-up.** Symja's first evaluation pays a large rule-engine
//!   initialisation, which used to get billed to whichever caller happened to
//!   be first and produced order-dependent timeout flakes. There is no rule
//!   engine here and no warm-up.
//! * **`SymjaOutputNormalizer`.** It rewrote `Log(`/`Log10(` in the output
//!   string before re-parsing. [`ops::display`] emits `ln(`/`log10(` directly.
//!
//! # The message contract
//!
//! `ReplEvaluator` renders a failure as `"CAS: " + message`, so these strings
//! are user-visible and are transcribed from the Java rather than reworded.
//! The one exception is the no-closed-form path: Symja signals it by echoing
//! the call back unevaluated and `ReplEvaluator.isUnevaluated` detects that by
//! string prefix, which is exactly the kind of guess this port removes.
//! [`CasError::NoClosedForm`] says it outright, and the boundary formats it as
//! `"<fn>: no closed form found for this input."` — the same sentence.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use num_traits::{One, ToPrimitive, Zero};

use crate::ast::Expr;
use crate::cas::ops;
use crate::cas::poly::Rat;

/// A CAS failure. Port of `CasEngine.CasException`, split into variants so the
/// boundary can tell "the user asked for something impossible" from "the input
/// was malformed".
///
/// Deliberately small: `clippy::result_large_err` is denied in this workspace,
/// and these travel in every `Result` the module returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CasError {
    /// A construct the CAS layer does not accept. Carries
    /// `ExprToSymja.UnsupportedExpression`'s exact wording.
    Unsupported(String),
    /// The expression string did not parse — `CasExpressions.ParseFailure`.
    Parse(String),
    /// A variable argument was not a plain identifier.
    InvalidVariable(String),
    /// The operation has no result inside this engine's scope. The boundary
    /// prints `"<fn>: no closed form found for this input."`.
    NoClosedForm { op: String },
    /// A division whose denominator is identically zero.
    DivisionByZero,
    /// The expression nests deeper than the engine will walk.
    TooDeep,
    /// An intermediate polynomial grew past the term cap.
    TooLarge,
    /// The symbolic identity could not be solved.
    Identity(String),
}

impl CasError {
    pub fn unsupported(message: impl Into<String>) -> CasError {
        CasError::Unsupported(message.into())
    }

    pub fn parse(message: impl Into<String>) -> CasError {
        CasError::Parse(message.into())
    }

    pub fn no_closed_form(op: impl Into<String>) -> CasError {
        CasError::NoClosedForm { op: op.into() }
    }

    pub fn division_by_zero() -> CasError {
        CasError::DivisionByZero
    }

    pub fn too_deep() -> CasError {
        CasError::TooDeep
    }

    pub fn too_large() -> CasError {
        CasError::TooLarge
    }
}

impl fmt::Display for CasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CasError::Unsupported(m) => f.write_str(m),
            CasError::Parse(m) => write!(f, "could not parse expression: {m}"),
            CasError::InvalidVariable(v) => write!(f, "invalid variable name: '{v}'"),
            CasError::NoClosedForm { op } => {
                write!(f, "{op}: no closed form found for this input.")
            }
            CasError::DivisionByZero => f.write_str("division by zero in a CAS expression"),
            CasError::TooDeep => f.write_str("CAS expression too deeply nested"),
            CasError::TooLarge => f.write_str("CAS expression grew too large to expand"),
            CasError::Identity(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for CasError {}

pub type CasResultOf<T> = std::result::Result<T, CasError>;

/// The result of one CAS operation. Port of `CasEngine.CasResult`.
///
/// `input` and `text` stand in for `symjaInput` / `symjaOutput`: the first is
/// still the Symja-shaped rendering of the *input* (a faithful port of
/// `ExprToSymja.convert`, kept because it is what the record carried and
/// because it is a useful diagnostic), the second is what `casDisplay` would
/// have produced — the string the REPL prints.
#[derive(Debug, Clone, PartialEq)]
pub struct CasResult {
    pub expr: Expr,
    pub latex: String,
    pub input: String,
    pub text: String,
    /// Ledger item 25: set when a ceiling (`MAX_POW`, `MAX_APART_DEGREE`)
    /// silently declined part of the work — "returned unfactored" must be
    /// distinguishable from "proved irreducible". `None` on a full answer.
    pub note: Option<String>,
}

/// The operations reachable from `ReplEvaluator.CAS_CALL`.
///
/// `LaplaceTransform` / `InverseLaplaceTransform` are the other two of the
/// thirteen; they live in [`crate::cas::laplace`] and the boundary routes them
/// there directly, because they need a transform table rather than the
/// rational core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Factor,
    Expand,
    Simplify,
    Together,
    Cancel,
    Numerator,
    Denominator,
    Apart,
    Collect,
    D,
    Integrate,
}

impl Op {
    /// The Symja head the Java used for this operation. It is what
    /// `ReplEvaluator` names in the "no closed form" message, so it is part of
    /// the observable behaviour.
    pub fn head(self) -> &'static str {
        match self {
            Op::Factor => "Factor",
            Op::Expand => "Expand",
            Op::Simplify => "Simplify",
            Op::Together => "Together",
            Op::Cancel => "Cancel",
            Op::Numerator => "Numerator",
            Op::Denominator => "Denominator",
            Op::Apart => "Apart",
            Op::Collect => "Collect",
            Op::D => "D",
            Op::Integrate => "Integrate",
        }
    }

    /// True when the REPL must supply a variable argument.
    pub fn needs_variable(self) -> bool {
        matches!(self, Op::Apart | Op::Collect | Op::D | Op::Integrate)
    }

    /// The lowercase REPL spelling, as matched by `ReplEvaluator.CAS_CALL`.
    pub fn from_repl_name(name: &str) -> Option<Op> {
        Some(match name.to_ascii_lowercase().as_str() {
            "factor" => Op::Factor,
            "expand" => Op::Expand,
            "simplify" => Op::Simplify,
            "together" => Op::Together,
            "cancel" => Op::Cancel,
            "numerator" => Op::Numerator,
            "denominator" => Op::Denominator,
            "apart" => Op::Apart,
            "collect" => Op::Collect,
            "diff" => Op::D,
            "integrate" => Op::Integrate,
            _ => return None,
        })
    }
}

// ── the CasEngine surface ───────────────────────────────────────────────────

/// Run a single-argument operation over an already-built expression.
pub fn apply_expr(op: Op, expr: &Expr) -> CasResultOf<CasResult> {
    ops::clear_ceiling_note();
    let result = match op {
        Op::Factor => ops::factor(expr)?,
        Op::Expand => ops::expand(expr)?,
        Op::Simplify => ops::simplify(expr)?,
        Op::Together => ops::together(expr)?,
        Op::Cancel => ops::cancel(expr)?,
        Op::Numerator => ops::numerator(expr)?,
        Op::Denominator => ops::denominator(expr)?,
        // `Op::needs_variable` is the caller's guard; reaching here means the
        // variable argument was dropped, which must not silently become a
        // no-op on an operation whose whole meaning depends on it.
        Op::Apart | Op::Collect | Op::D | Op::Integrate => {
            return Err(CasError::unsupported(format!(
                "{} needs a variable: {}(expr, var)",
                op.head(),
                op.head()
            )))
        }
    };
    build(expr, result)
}

/// Run a `(expression, variable)` operation over an already-built expression.
pub fn apply_expr_with_variable(op: Op, expr: &Expr, variable: &str) -> CasResultOf<CasResult> {
    ops::clear_ceiling_note();
    let var = require_identifier(variable)?;
    let result = match op {
        Op::Apart => ops::apart(expr, &var)?,
        Op::Collect => ops::collect(expr, &var)?,
        Op::D => ops::differentiate(expr, &var)?,
        Op::Integrate => ops::integrate(expr, &var)?,
        // The Java accepts the same call shape for the single-argument heads
        // and simply ignores the extra argument; doing the same keeps the REPL
        // dispatch from needing two code paths.
        other => return apply_expr(other, expr),
    };
    build(expr, result)
}

/// Run a single-argument operation over an expression *string*, exactly as
/// `CasEngine.apply` does.
pub fn apply(op: Op, expression: &str) -> CasResultOf<CasResult> {
    apply_expr(op, &parse_expression(expression)?)
}

/// Run a `(expression, variable)` operation over an expression string.
pub fn apply_with_variable(op: Op, expression: &str, variable: &str) -> CasResultOf<CasResult> {
    apply_expr_with_variable(op, &parse_expression(expression)?, variable)
}

pub fn factor(expression: &str) -> CasResultOf<CasResult> {
    apply(Op::Factor, expression)
}

pub fn expand(expression: &str) -> CasResultOf<CasResult> {
    apply(Op::Expand, expression)
}

pub fn simplify(expression: &str) -> CasResultOf<CasResult> {
    apply(Op::Simplify, expression)
}

/// Partial-fraction decomposition — the Laplace residue workflow.
pub fn apart(expression: &str, variable: &str) -> CasResultOf<CasResult> {
    apply_with_variable(Op::Apart, expression, variable)
}

/// Partial fractions of an already-built tree, the overload
/// `CasEngineTest.apartAcceptsBuiltTransferFunction` exercises.
pub fn apart_expr(expression: &Expr, variable: &str) -> CasResultOf<CasResult> {
    apply_expr_with_variable(Op::Apart, expression, variable)
}

fn build(input: &Expr, result: Expr) -> CasResultOf<CasResult> {
    let text = ops::display(&result);
    let symja_input = ops::to_symja_input(input)?;
    let latex = crate::parser::latex::expr_to_latex(&result);
    Ok(CasResult {
        expr: result,
        latex,
        input: symja_input,
        text,
        // Whatever ceiling fired during the lowering above (cleared at the
        // op entry points, so it can only be this operation's).
        note: ops::take_ceiling_note(),
    })
}

/// Variable names must be plain identifiers, lowercased — a transcription of
/// `CasEngine.requireIdentifier`, including its message.
pub fn require_identifier(variable: &str) -> CasResultOf<String> {
    let mut chars = variable.chars();
    let ok = match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => chars.all(|c| c.is_ascii_alphanumeric() || c == '_'),
        _ => false,
    };
    if !ok {
        return Err(CasError::InvalidVariable(variable.to_string()));
    }
    Ok(variable.to_ascii_lowercase())
}

/// Parse a single frees expression — the port of `CasExpressions.parse`.
///
/// Like the Java it anchors on end-of-input, because the `expr` rule does not:
/// without the check `"x +"` would parse as `x` and silently drop the rest.
pub fn parse_expression(source: &str) -> CasResultOf<Expr> {
    let tokens = crate::lexer::tokenize(source).map_err(|e| CasError::parse(e.to_string()))?;
    let mut cursor = crate::parser::Cursor::new(&tokens, source);
    let expr =
        crate::parser::parse_expr(&mut cursor).map_err(|e| CasError::parse(e.to_string()))?;
    cursor.skip_newlines();
    if !cursor.is_eof() {
        let text = cursor.span().slice(source);
        return Err(CasError::parse(format!(
            "unexpected trailing input: '{text}'"
        )));
    }
    Ok(expr)
}

// ── CasIdentity ─────────────────────────────────────────────────────────────

/// One term of the residual numerator, with the power of the identity variable
/// already stripped off: the coefficient, and what is left of the monomial.
/// Anything but a single unknown to the first power makes the system nonlinear.
type IdentityTerm = (Rat, Vec<(String, u32)>);

/// Solve `lhs == rhs` as an identity in `variable`, for every other symbol.
///
/// Port of `CasIdentity.solveCoefficients`. This is what a `SYMBOLIC`
/// declaration feeds: writing `(s+3)/(s^2+3*s+2) = A/(s+1) + B/(s+2)` in a
/// document solves for the residues `A = 2`, `B = -1`, which then become
/// ordinary frees variables.
///
/// Method, unchanged from the Java: the identity holds for all `variable`
/// exactly when the numerator of `lhs - rhs` over a common denominator is the
/// zero polynomial, so every coefficient of `variable` must vanish. The Java
/// then hands those equations to Symja's general `Solve`; its own doc comment
/// records that they are **linear in the unknowns**, so this port solves them
/// with exact Gaussian elimination over ℚ and refuses anything nonlinear by
/// name instead of silently attempting more.
pub fn solve_coefficients(
    lhs: &Expr,
    rhs: &Expr,
    variable: &str,
) -> CasResultOf<BTreeMap<String, f64>> {
    solve_coefficients_with(lhs, rhs, variable, |e| Ok(e.clone()))
}

/// [`solve_coefficients`] with a pre-pass over both sides.
///
/// The Java runs `TransferFunction.expandCalls` first so an identity can be
/// written `tf([1,3],[1,3,2]) = A/(s+1) + B/(s+2)`. `TransferFunction` is the
/// control suite's, not the CAS's, so it arrives as a hook rather than a
/// hard dependency — the boundary passes `control::tf`'s expander and this
/// module stays independent of it.
pub fn solve_coefficients_with(
    lhs: &Expr,
    rhs: &Expr,
    variable: &str,
    pre: impl Fn(&Expr) -> CasResultOf<Expr>,
) -> CasResultOf<BTreeMap<String, f64>> {
    let var = variable.to_ascii_lowercase();
    let lhs = pre(lhs)?;
    let rhs = pre(rhs)?;

    let mut unknowns: BTreeSet<String> = lhs.variables();
    unknowns.extend(rhs.variables());
    unknowns.remove(&var);
    if unknowns.is_empty() {
        return Err(CasError::Identity(
            "identity has no unknown coefficients to solve for".into(),
        ));
    }
    let unknowns: Vec<String> = unknowns.into_iter().collect();

    // numerator(Together(lhs - rhs)) must be the zero polynomial in `var`.
    let difference = Expr::bin(crate::ast::BinOp::Sub, lhs, rhs);
    let mut gens = ops::Gens::new();
    let residual = ops::lower(&difference, &mut gens)?;
    let numerator = residual.numer().clone();

    // Group the numerator's terms by the power of `var`; each group is one
    // linear equation in the unknowns.
    let vars = numerator.vars().to_vec();
    let mut rows: BTreeMap<u32, Vec<IdentityTerm>> = BTreeMap::new();
    for (mono, coeff) in numerator.terms() {
        if coeff.is_zero() {
            continue;
        }
        let mut power = 0;
        let mut rest: Vec<(String, u32)> = Vec::new();
        for (i, name) in vars.iter().enumerate() {
            let exponent = mono.exponent(i);
            if exponent == 0 {
                continue;
            }
            if *name == var {
                power = exponent;
            } else {
                rest.push((name.clone(), exponent));
            }
        }
        rows.entry(power).or_default().push((coeff.clone(), rest));
    }

    let mut matrix: Vec<Vec<Rat>> = Vec::new();
    let mut targets: Vec<Rat> = Vec::new();
    for terms in rows.values() {
        let mut row = vec![Rat::zero(); unknowns.len()];
        let mut constant = Rat::zero();
        for (coeff, rest) in terms {
            match rest.len() {
                0 => constant += coeff,
                1 if rest[0].1 == 1 => {
                    let Some(index) = unknowns.iter().position(|u| *u == rest[0].0) else {
                        return Err(unsolved(&unknowns));
                    };
                    row[index] += coeff;
                }
                // A term of degree ≥ 2 in the unknowns, or one carrying a
                // generator that is not an unknown at all.
                _ => return Err(unsolved(&unknowns)),
            }
        }
        matrix.push(row);
        targets.push(-constant);
    }

    let Some(solution) = solve_linear(&mut matrix, &mut targets, unknowns.len()) else {
        return Err(unsolved(&unknowns));
    };
    Ok(unknowns
        .iter()
        .cloned()
        .zip(solution.iter().map(rat_to_f64))
        .collect())
}

fn rat_to_f64(r: &Rat) -> f64 {
    r.to_f64().unwrap_or(f64::NAN)
}

fn unsolved(unknowns: &[String]) -> CasError {
    CasError::Identity(format!(
        "could not solve the identity for: {} (it may be inconsistent or underdetermined)",
        unknowns.join(", ")
    ))
}

/// Exact Gaussian elimination over ℚ.
///
/// Returns `None` when the system is inconsistent or leaves any unknown free —
/// the two cases the Java reports as "inconsistent or underdetermined". An
/// over-determined but consistent system (more coefficient equations than
/// unknowns, which is the normal shape here) solves fine.
///
/// Shared with [`crate::cas::ops`], whose partial-fraction decomposition solves
/// the same kind of square system for the residue numerators.
pub(crate) fn solve_linear(a: &mut [Vec<Rat>], b: &mut [Rat], unknowns: usize) -> Option<Vec<Rat>> {
    let rows = a.len();
    let mut pivot_of = vec![usize::MAX; unknowns];
    let mut row = 0;
    for column in 0..unknowns {
        let Some(found) = (row..rows).find(|r| !a[*r][column].is_zero()) else {
            continue;
        };
        a.swap(row, found);
        b.swap(row, found);
        let inverse = Rat::one() / a[row][column].clone();
        for cell in a[row].iter_mut().take(unknowns).skip(column) {
            *cell *= &inverse;
        }
        b[row] *= &inverse;
        let pivot_row = a[row].clone();
        for r in 0..rows {
            if r == row || a[r][column].is_zero() {
                continue;
            }
            let factor = a[r][column].clone();
            for (cell, above) in a[r]
                .iter_mut()
                .zip(pivot_row.iter())
                .take(unknowns)
                .skip(column)
            {
                *cell -= above * &factor;
            }
            let delta = &b[row] * &factor;
            b[r] -= delta;
        }
        pivot_of[column] = row;
        row += 1;
    }
    // Any row that is now all zeros on the left but not on the right is an
    // inconsistency (`1/(s+1) = A/(s+2)` produces exactly that).
    for r in 0..rows {
        if a[r].iter().take(unknowns).all(Zero::is_zero) && !b[r].is_zero() {
            return None;
        }
    }
    if pivot_of.contains(&usize::MAX) {
        return None;
    }
    Some(pivot_of.iter().map(|p| b[*p].clone()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cas::poly::{rat, rat_int};

    fn parse(source: &str) -> Expr {
        parse_expression(source).expect("parses")
    }

    #[test]
    fn factor_returns_the_engines_own_spelling() {
        let r = factor("x^2 - 1").expect("ok");
        assert_eq!(r.text, "(-1+x)*(1+x)");
        assert_eq!(r.input, "((x^2)-1)");
        assert!(!r.latex.is_empty());
    }

    #[test]
    fn apart_accepts_a_prebuilt_tree() {
        // The Java's `apart(Expr, var)` overload, reached from
        // `TransferFunction.fraction`.
        let tf = parse("(s + 3)/(s^2 + 3*s + 2)");
        let r = apart_expr(&tf, "s").expect("ok");
        assert_eq!(r.text, "2/(1+s)-1/(2+s)");
    }

    #[test]
    fn variable_names_must_be_identifiers() {
        assert_eq!(require_identifier("S").expect("ok"), "s");
        assert_eq!(require_identifier("x_1").expect("ok"), "x_1");
        assert_eq!(
            apart("(s+3)/(s^2+3*s+2)", "1x")
                .expect_err("refused")
                .to_string(),
            "invalid variable name: '1x'"
        );
    }

    #[test]
    fn unparseable_input_is_refused_like_the_java() {
        let err = factor("x +").expect_err("refused");
        assert!(
            err.to_string().starts_with("could not parse expression:"),
            "{err}"
        );
        assert!(factor("[1, 2, 3]").is_err());
    }

    #[test]
    fn integrate_outside_the_table_names_itself() {
        let err = apply_with_variable(Op::Integrate, "exp(x^2)", "x").expect_err("refused");
        assert_eq!(
            err.to_string(),
            "Integrate: no closed form found for this input."
        );
    }

    #[test]
    fn repl_names_map_to_operations() {
        assert_eq!(Op::from_repl_name("Factor"), Some(Op::Factor));
        assert_eq!(Op::from_repl_name("diff"), Some(Op::D));
        assert_eq!(Op::from_repl_name("ilaplace"), None);
        assert!(Op::Apart.needs_variable());
        assert!(!Op::Factor.needs_variable());
        assert_eq!(Op::D.head(), "D");
    }

    #[test]
    fn dropping_a_required_variable_is_refused_not_ignored() {
        let err = apply_expr(Op::Apart, &parse("1/(s^2-1)")).expect_err("refused");
        assert_eq!(err.to_string(), "Apart needs a variable: Apart(expr, var)");
    }

    // ── CasIdentity, against CasIdentityTest ────────────────────────────────

    fn solve(lhs: &str, rhs: &str, var: &str) -> CasResultOf<BTreeMap<String, f64>> {
        solve_coefficients(&parse(lhs), &parse(rhs), var)
    }

    #[test]
    fn solves_partial_fraction_residues() {
        let c = solve("(s + 3)/(s^2 + 3*s + 2)", "a/(s+1) + b/(s+2)", "s").expect("ok");
        assert!((c["a"] - 2.0).abs() < 1e-9, "{c:?}");
        assert!((c["b"] + 1.0).abs() < 1e-9, "{c:?}");
    }

    #[test]
    fn solves_residues_with_a_pole_at_the_origin() {
        let c = solve("(2*s + 1)/(s^2 + s)", "a/s + b/(s+1)", "s").expect("ok");
        assert!((c["a"] - 1.0).abs() < 1e-9, "{c:?}");
        assert!((c["b"] - 1.0).abs() < 1e-9, "{c:?}");
    }

    #[test]
    fn solves_repeated_pole_residues() {
        let c = solve("(s + 3)/(s^2 + 2*s + 1)", "a/(s+1) + b/(s+1)^2", "s").expect("ok");
        assert!((c["a"] - 1.0).abs() < 1e-9, "{c:?}");
        assert!((c["b"] - 2.0).abs() < 1e-9, "{c:?}");
    }

    #[test]
    fn solves_an_irreducible_quadratic_term() {
        let c = solve(
            "(s^2 + 2)/((s+1)*(s^2 + s + 1))",
            "a/(s+1) + (b*s + d)/(s^2 + s + 1)",
            "s",
        )
        .expect("ok");
        let (a, b, d) = (c["a"], c["b"], c["d"]);
        for s in [-3.0_f64, 0.0, 2.0, 5.5] {
            let lhs = (s * s + 2.0) / ((s + 1.0) * (s * s + s + 1.0));
            let rhs = a / (s + 1.0) + (b * s + d) / (s * s + s + 1.0);
            assert!((lhs - rhs).abs() < 1e-9, "mismatch at s={s}");
        }
    }

    #[test]
    fn rejects_an_inconsistent_identity() {
        // 1/(s+1) can never equal A/(s+2) for all s.
        let err = solve("1/(s+1)", "a/(s+2)", "s").expect_err("refused");
        assert!(
            err.to_string().contains("could not solve the identity"),
            "{err}"
        );
    }

    #[test]
    fn rejects_an_identity_with_nothing_to_solve_for() {
        let err = solve("1/(s+1)", "1/(s+1)", "s").expect_err("refused");
        assert_eq!(
            err.to_string(),
            "identity has no unknown coefficients to solve for"
        );
    }

    #[test]
    fn rejects_a_nonlinear_identity_by_name() {
        // A*B is quadratic in the unknowns; the Java would hand it to Symja's
        // general Solve, this port declines rather than half-solving it.
        let err = solve("s + 1", "a*b*s + 1", "s").expect_err("refused");
        assert!(
            err.to_string().contains("could not solve the identity"),
            "{err}"
        );
    }

    #[test]
    fn identity_hook_runs_before_the_solve() {
        // The `TransferFunction.expandCalls` seam: a pre-pass may rewrite both
        // sides. Substituting the same fraction the Java's `tf(...)` expands to
        // must give the documented residues.
        let lhs = parse("tf_placeholder");
        let rhs = parse("a/(s+1) + b/(s+2)");
        let c = solve_coefficients_with(&lhs, &rhs, "s", |e| {
            if *e == parse("tf_placeholder") {
                Ok(parse("(s + 3)/(s^2 + 3*s + 2)"))
            } else {
                Ok(e.clone())
            }
        })
        .expect("ok");
        assert!((c["a"] - 2.0).abs() < 1e-9, "{c:?}");
        assert!((c["b"] + 1.0).abs() < 1e-9, "{c:?}");
    }

    #[test]
    fn gaussian_elimination_is_exact() {
        // 2a + b = 1, a - b = 1  ->  a = 2/3, b = -1/3. Guards the solver both
        // the identity path and `ops::apart` lean on.
        let mut matrix = vec![vec![rat_int(2), rat_int(1)], vec![rat_int(1), rat_int(-1)]];
        let mut targets = vec![rat_int(1), rat_int(1)];
        let solution = solve_linear(&mut matrix, &mut targets, 2).expect("solvable");
        assert_eq!(solution[0], rat(2, 3).expect("rational"));
        assert_eq!(solution[1], rat(-1, 3).expect("rational"));
    }

    #[test]
    fn gaussian_elimination_refuses_a_free_unknown() {
        let mut matrix = vec![vec![rat_int(1), rat_int(0)]];
        let mut targets = vec![rat_int(1)];
        assert!(solve_linear(&mut matrix, &mut targets, 2).is_none());
    }
}
