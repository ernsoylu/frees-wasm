//! Equation-based integral support.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/core/IntegralSolver.java`
//! (439 LOC) — `F = Integral(f, t, a, b[, step])` and the `GaussIntegral`
//! quadrature intrinsic. **Phase-4 contract stub** — the integral agent
//! replaces the bodies; the evaluator dispatches to them in parallel.

use crate::ast::Expr;
use crate::diag::{FreesError, Result};
use crate::eval::Scope;

/// `GaussIntegral(f, t, a, b[, points])`: fixed-order Gauss–Legendre
/// quadrature of `f` over `t ∈ [a, b]`, with `t` bound inside `f` only
/// (mirroring `Expr::variables`' special case).
pub fn gauss_integral(
    integrand: &Expr,
    var: &str,
    a: f64,
    b: f64,
    points: Option<usize>,
    scope: &Scope,
) -> Result<f64> {
    let _ = (integrand, var, a, b, points, scope);
    Err(FreesError::evaluation(
        "GaussIntegral is not yet supported by the wasm engine",
    ))
}

/// `Integral(f, t, a, b[, step])` — the equation-based integral.
pub fn integral(
    integrand: &Expr,
    var: &str,
    a: f64,
    b: f64,
    step: Option<f64>,
    scope: &Scope,
) -> Result<f64> {
    let _ = (integrand, var, a, b, step, scope);
    Err(FreesError::evaluation(
        "Integral is not yet supported by the wasm engine",
    ))
}
