//! Levenberg–Marquardt least-squares curve fitting.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/core/CurveFitter.java`
//! (288 LOC) **plus** the optimizer it delegates to, Commons Math 3.6.1's
//! `org.apache.commons.math3.fitting.leastsquares.LevenbergMarquardtOptimizer`
//! — itself a transcription of MINPACK's `lmder`. The optimizer had to come
//! along: the fit reports `iterations`, and the damping schedule, the
//! column-norm scaling and the four convergence tests together decide both the
//! iteration count and the exact path to the optimum. A different LM converges
//! to the same optimum by a different route and reproduces neither.
//!
//! # The pipeline
//!
//! `y = a * exp(-b * x) + c` is parsed, the dependent variable is matched
//! against one side, and the *other* side becomes the model expression. The
//! parameters `[a, b, c]` are then fitted to the observed `(x, y)` pairs by
//! minimising `Σ (yᵢ − model(xᵢ; p))²`. The Jacobian handed to the optimizer is
//! central finite differences of the model with the step
//! `h = max(1e-8, |pⱼ| · 1e-8)` — the Java `FD_STEP` rule, which is *not* the
//! forward-difference rule the uncertainty engine uses.
//!
//! # Parameter bounds are accepted by the Java signature and ignored
//!
//! `CurveFitter.fit` takes `lowerBounds` / `upperBounds` and its body contains
//! an empty `if` with the comment "LevenbergMarquardtOptimizer doesn't directly
//! support bounds in Commons Math 3.x, so we proceed without box constraints".
//! They are therefore not part of this API — `CurveFitParams` in
//! `web/src/api.ts` does not send them either. Adding real box constraints here
//! would be a behaviour change, not a port.

// The LM core below is a line-by-line transcription of Commons Math's
// `LevenbergMarquardtOptimizer` (MINPACK `lmder`), an algorithm written
// entirely in terms of a column permutation: nearly every loop indexes
// `permutation[k]` rather than walking a slice in order. Rewriting the index
// arithmetic into iterator form would destroy the line-for-line correspondence
// with the reference and make a numerical divergence impossible to spot in
// review, so the range-loop lints are turned off for this module only.
#![allow(clippy::needless_range_loop)]

use std::collections::HashMap;

use crate::ast::{Equation, Expr};
use crate::diag::{FreesError, Result};
use crate::eval::{eval, Scope};
use crate::linalg::Mat;

/// `CurveFitter.MAX_EVALUATIONS`.
const MAX_EVALUATIONS: usize = 10_000;
/// `CurveFitter.MAX_ITERATIONS`.
const MAX_ITERATIONS: usize = 1_000;
/// `CurveFitter.FD_STEP` — the central-difference step scale.
const FD_STEP: f64 = 1e-8;

/// `LevenbergMarquardtOptimizer`'s default configuration: initial step-bound
/// factor 100, cost/parameter/orthogonality tolerances 1e-10, and
/// `Precision.SAFE_MIN` as the QR ranking threshold.
const INITIAL_STEP_BOUND_FACTOR: f64 = 100.0;
const COST_RELATIVE_TOLERANCE: f64 = 1e-10;
const PAR_RELATIVE_TOLERANCE: f64 = 1e-10;
const ORTHO_TOLERANCE: f64 = 1e-10;
/// `Precision.SAFE_MIN` = `0x1.0p-1022`.
const SAFE_MIN: f64 = f64::MIN_POSITIVE;
/// `2 * Precision.EPSILON`, where Commons Math's `EPSILON` is `0x1.0p-53`.
/// This is therefore `0x1.0p-52`, i.e. `f64::EPSILON`.
const TWO_EPS: f64 = f64::EPSILON;

/// The outcome of a curve-fit run. Port of `CurveFitter.FitResult`.
#[derive(Debug, Clone, PartialEq)]
pub struct FitResult {
    pub fitted_parameters: Vec<f64>,
    /// The parameter names, **lowercased** — the Java lowercases them to match
    /// the AST convention and reports the lowercased list.
    pub parameter_names: Vec<String>,
    /// `1 − SSres/SStot`, or exactly `1.0` when the observations have zero
    /// spread (the Java `ssTot == 0.0` guard).
    pub r_squared: f64,
    /// `sqrt(SSres / n)` — divided by `n`, not by the degrees of freedom.
    pub rmse: f64,
    /// Optimizer iterations, i.e. Commons Math's `Optimum.getIterations()`.
    pub iterations: usize,
    /// `observed − fitted`, per data point.
    pub residuals: Vec<f64>,
    pub fitted_values: Vec<f64>,
}

/// Fits `model` to the observed `(x, y)` data. Port of `CurveFitter.fit`.
///
/// * `model` — the model equation, e.g. `"y = a * exp(-b * x) + c"`.
/// * `y_variable` / `x_variable` — the dependent and independent variable
///   names, any case.
/// * `parameters` — the names to fit, any case; reported lowercased.
/// * `initial_guess` — starting values. Entries past the end of the slice (and
///   a `None` slice) default to `1.0`, matching the Java's per-index
///   `initialGuess != null && i < size && get(i) != null` test. The Java's
///   third clause — a `null` *inside* the list — has no counterpart in
///   `&[f64]`; a caller that wants the default for one parameter passes a
///   shorter slice or `1.0`.
///
/// # Errors
///
/// * [`FreesError::Solver`] — a validation failure (blank model, empty or
///   mismatched data, no parameters), a model whose dependent variable is not
///   alone on one side, or an optimizer that hit its evaluation/iteration
///   budget or a Commons Math `ConvergenceException`.
/// * [`FreesError::Parse`] — the model equation does not parse.
pub fn fit(
    model: &str,
    y_variable: &str,
    x_variable: &str,
    parameters: &[String],
    x_data: &[f64],
    y_data: &[f64],
    initial_guess: Option<&[f64]>,
) -> Result<FitResult> {
    validate_fit_inputs(model, x_data, y_data, parameters)?;

    let n = x_data.len();
    let p = parameters.len();

    let model_expr = parse_model_expression(model, y_variable)?;

    // The AST stores identifiers lowercase, so the fit works in that alphabet.
    let param_lower: Vec<String> = parameters
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let x_var_lower = x_variable.to_ascii_lowercase();

    let start: Vec<f64> = (0..p)
        .map(|i| initial_guess.and_then(|g| g.get(i).copied()).unwrap_or(1.0))
        .collect();

    let observed = y_data.to_vec();

    // The model + central-difference Jacobian, i.e. `buildModelFunction`.
    let model_function = |params: &[f64]| -> (Vec<f64>, Mat) {
        let mut values = vec![0.0; n];
        let mut jacobian = vec![vec![0.0; p]; n];
        for i in 0..n {
            let xi = x_data[i];
            values[i] = evaluate(&model_expr, &x_var_lower, xi, &param_lower, params);
            for j in 0..p {
                let h = FD_STEP.max(params[j].abs() * FD_STEP);
                let mut plus = params.to_vec();
                let mut minus = params.to_vec();
                plus[j] += h;
                minus[j] -= h;
                let f_plus = evaluate(&model_expr, &x_var_lower, xi, &param_lower, &plus);
                let f_minus = evaluate(&model_expr, &x_var_lower, xi, &param_lower, &minus);
                jacobian[i][j] = (f_plus - f_minus) / (2.0 * h);
            }
        }
        (values, jacobian)
    };

    let optimum = levenberg_marquardt(&start, &observed, model_function)?;
    let fitted = optimum.point;
    let iterations = optimum.iterations;

    // Fitted values, residuals, R² and RMSE — recomputed at the optimum rather
    // than reused from the optimizer's last evaluation, as the Java does.
    let mut fitted_values = vec![0.0; n];
    let mut residuals = vec![0.0; n];
    let mut ss_res = 0.0;
    let y_mean = observed.iter().sum::<f64>() / n as f64;
    let mut ss_tot = 0.0;
    for i in 0..n {
        fitted_values[i] = evaluate(&model_expr, &x_var_lower, x_data[i], &param_lower, &fitted);
        residuals[i] = observed[i] - fitted_values[i];
        ss_res += residuals[i] * residuals[i];
        ss_tot += (observed[i] - y_mean) * (observed[i] - y_mean);
    }

    let r_squared = if ss_tot == 0.0 {
        1.0
    } else {
        1.0 - ss_res / ss_tot
    };
    let rmse = (ss_res / n as f64).sqrt();

    Ok(FitResult {
        fitted_parameters: fitted,
        parameter_names: param_lower,
        r_squared,
        rmse,
        iterations,
        residuals,
        fitted_values,
    })
}

/// Port of `CurveFitter.validateFitInputs`, message for message.
fn validate_fit_inputs(
    model: &str,
    x_data: &[f64],
    y_data: &[f64],
    parameters: &[String],
) -> Result<()> {
    if model.trim().is_empty() {
        return Err(FreesError::solver("Model equation is required."));
    }
    if x_data.is_empty() {
        return Err(FreesError::solver("Data points are required."));
    }
    if x_data.len() != y_data.len() {
        return Err(FreesError::solver(format!(
            "x and y data must have the same length (got {} and {}).",
            x_data.len(),
            y_data.len()
        )));
    }
    if parameters.is_empty() {
        return Err(FreesError::solver(
            "At least one parameter to fit is required.",
        ));
    }
    Ok(())
}

/// Parses the model equation and returns the side that is *not* the dependent
/// variable. Port of `CurveFitter.parseModelExpression`, including its refusal
/// to guess when neither side is the bare dependent variable.
fn parse_model_expression(model: &str, y_variable: &str) -> Result<Expr> {
    let doc = crate::parser::parse_document(model)?;
    let equations: Vec<Equation> = crate::parser::expand::expand_document(&doc)?;
    let Some(eq) = equations.into_iter().next() else {
        return Err(FreesError::solver("Model equation could not be parsed."));
    };

    let y_lower = y_variable.to_ascii_lowercase();
    if matches!(&eq.lhs, Expr::Var(name) if *name == y_lower) {
        return Ok(eq.rhs);
    }
    if matches!(&eq.rhs, Expr::Var(name) if *name == y_lower) {
        return Ok(eq.lhs);
    }
    Err(FreesError::solver(format!(
        "Could not identify '{y_variable}' as the dependent variable in the model equation. \
         Expected a form like '{y_variable} = <expression>'."
    )))
}

/// Evaluates the model at one data point. Port of `CurveFitter.evaluate`: an
/// evaluation failure becomes `NaN` rather than an error, so the optimizer sees
/// a hostile point instead of a crash.
///
/// The Java uses the two-argument `Evaluator.eval(expr, values)` — no `defs` —
/// so a model equation cannot call a document `FUNCTION` or `TABLE`. Mirrored.
fn evaluate(
    expr: &Expr,
    x_var: &str,
    x_value: f64,
    param_names: &[String],
    param_values: &[f64],
) -> f64 {
    let mut scope: Scope = HashMap::with_capacity(param_names.len() + 1);
    scope.insert(x_var.to_string(), x_value);
    for (name, value) in param_names.iter().zip(param_values) {
        scope.insert(name.clone(), *value);
    }
    eval(expr, &scope).unwrap_or(f64::NAN)
}

// ---------------------------------------------------------------------------
// Commons Math `LevenbergMarquardtOptimizer` (MINPACK lmder)
// ---------------------------------------------------------------------------

/// One evaluation of the least-squares problem: the point, the residuals
/// `target − model(point)`, the cost `‖residuals‖₂` and the model Jacobian.
/// Commons Math's `LeastSquaresProblem.Evaluation` for the unweighted case.
#[derive(Clone)]
struct LmEvaluation {
    point: Vec<f64>,
    residuals: Vec<f64>,
    cost: f64,
    jacobian: Mat,
}

/// What `Optimum` carries back out of the optimizer.
struct LmOptimum {
    point: Vec<f64>,
    iterations: usize,
}

/// The QR-with-column-pivoting state Commons Math calls `InternalData`.
struct InternalData {
    /// **Negated** Jacobian, overwritten in place by the Householder
    /// reflections — the Java comment reads "Code in this class assumes that the
    /// weighted Jacobian is -(W^(1/2) J), hence the multiplication by -1".
    weighted_jacobian: Mat,
    permutation: Vec<usize>,
    rank: usize,
    diag_r: Vec<f64>,
    jac_norm: Vec<f64>,
    beta: Vec<f64>,
}

/// The scratch vectors the Java allocates once per `optimize` call.
struct LmScratch {
    lm_dir: Vec<f64>,
    diag: Vec<f64>,
    old_x: Vec<f64>,
    qtf: Vec<f64>,
    work1: Vec<f64>,
    work2: Vec<f64>,
    work3: Vec<f64>,
}

/// Runs Commons Math's `LevenbergMarquardtOptimizer.optimize`.
///
/// `model(point) -> (values, jacobian)` is the `MultivariateJacobianFunction`;
/// residuals are `target − values`, exactly as `computeResiduals` defines them.
fn levenberg_marquardt<F>(start: &[f64], target: &[f64], mut model: F) -> Result<LmOptimum>
where
    F: FnMut(&[f64]) -> (Vec<f64>, Mat),
{
    let n_r = target.len();
    let n_c = start.len();
    let solved_cols = n_r.min(n_c);

    let mut scratch = LmScratch {
        lm_dir: vec![0.0; n_c],
        diag: vec![0.0; n_c],
        old_x: vec![0.0; n_c],
        qtf: vec![0.0; n_r],
        work1: vec![0.0; n_c],
        work2: vec![0.0; n_c],
        work3: vec![0.0; n_c],
    };
    let mut lm_par = 0.0f64;
    let mut delta = 0.0f64;
    let mut x_norm = 0.0f64;
    let mut old_res = vec![0.0; n_r];

    let mut evaluations = 0usize;
    let mut iterations = 0usize;

    let mut evaluate_at = |point: &[f64], counter: &mut usize| -> Result<LmEvaluation> {
        *counter += 1;
        if *counter > MAX_EVALUATIONS {
            return Err(FreesError::solver(format!(
                "Curve fit gave up: more than {MAX_EVALUATIONS} model evaluations."
            )));
        }
        let (values, jacobian) = model(point);
        let residuals: Vec<f64> = target
            .iter()
            .zip(&values)
            .map(|(t, v)| t - v)
            .collect::<Vec<_>>();
        let cost = residuals.iter().map(|r| r * r).sum::<f64>().sqrt();
        Ok(LmEvaluation {
            point: point.to_vec(),
            residuals,
            cost,
            jacobian,
        })
    };

    let mut current = evaluate_at(start, &mut evaluations)?;
    let mut weighted_residual = current.residuals.clone();
    // Java keeps `currentPoint` as an array of its own, separate from
    // `current.getPoint()`. That matters when there are more parameters than
    // data points: a failed step restores only the first `solvedCols` permuted
    // components, and the rest keep the rejected step's values.
    let mut current_point = current.point.clone();

    let mut first_iteration = true;
    loop {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            return Err(FreesError::solver(format!(
                "Curve fit gave up: more than {MAX_ITERATIONS} iterations."
            )));
        }
        let previous = current.clone();

        let mut data = qr_decomposition(&current.jacobian, solved_cols, n_r, n_c)?;

        scratch.qtf[..n_r].copy_from_slice(&weighted_residual[..n_r]);
        q_t_y(&mut scratch.qtf, &data, n_r, n_c);

        // Q is no longer needed; let the Jacobian hold R with its diagonal.
        for k in 0..solved_cols {
            let pk = data.permutation[k];
            data.weighted_jacobian[k][pk] = data.diag_r[pk];
        }

        if first_iteration {
            // Scale the point by the initial Jacobian's column norms.
            x_norm = 0.0;
            for k in 0..n_c {
                let mut dk = data.jac_norm[k];
                if dk == 0.0 {
                    dk = 1.0;
                }
                let xk = dk * current_point[k];
                x_norm += xk * xk;
                scratch.diag[k] = dk;
            }
            x_norm = x_norm.sqrt();
            delta = if x_norm == 0.0 {
                INITIAL_STEP_BOUND_FACTOR
            } else {
                INITIAL_STEP_BOUND_FACTOR * x_norm
            };
        }

        // Orthogonality between the residual vector and the Jacobian columns.
        let mut max_cosine = 0.0f64;
        if current.cost != 0.0 {
            for j in 0..solved_cols {
                let pj = data.permutation[j];
                let s = data.jac_norm[pj];
                if s != 0.0 {
                    let mut sum = 0.0;
                    for i in 0..=j {
                        sum += data.weighted_jacobian[i][pj] * scratch.qtf[i];
                    }
                    max_cosine = max_cosine.max(sum.abs() / (s * current.cost));
                }
            }
        }
        if max_cosine <= ORTHO_TOLERANCE {
            return Ok(LmOptimum {
                point: current.point,
                iterations,
            });
        }

        for j in 0..n_c {
            scratch.diag[j] = scratch.diag[j].max(data.jac_norm[j]);
        }

        // Inner loop: shrink the trust region until a step is accepted.
        let mut ratio = 0.0f64;
        while ratio < 1.0e-4 {
            for j in 0..solved_cols {
                let pj = data.permutation[j];
                scratch.old_x[pj] = current_point[pj];
            }
            let previous_cost = current.cost;
            std::mem::swap(&mut weighted_residual, &mut old_res);

            lm_par = determine_lm_parameter(
                &scratch.qtf.clone(),
                delta,
                &mut scratch,
                &mut data,
                solved_cols,
                lm_par,
            );

            // The new point, and the norm of the step that reaches it.
            let mut lm_norm = 0.0;
            for j in 0..solved_cols {
                let pj = data.permutation[j];
                scratch.lm_dir[pj] = -scratch.lm_dir[pj];
                current_point[pj] = scratch.old_x[pj] + scratch.lm_dir[pj];
                let s = scratch.diag[pj] * scratch.lm_dir[pj];
                lm_norm += s * s;
            }
            let lm_norm = lm_norm.sqrt();
            if first_iteration {
                delta = delta.min(lm_norm);
            }

            current = evaluate_at(&current_point, &mut evaluations)?;
            weighted_residual = current.residuals.clone();
            current_point.clone_from(&current.point);
            let current_cost = current.cost;

            // Scaled actual reduction.
            let mut act_red = -1.0;
            if 0.1 * current_cost < previous_cost {
                let r = current_cost / previous_cost;
                act_red = 1.0 - r * r;
            }

            // Scaled predicted reduction and directional derivative.
            for j in 0..solved_cols {
                let pj = data.permutation[j];
                let dir_j = scratch.lm_dir[pj];
                scratch.work1[j] = 0.0;
                for i in 0..=j {
                    scratch.work1[i] += data.weighted_jacobian[i][pj] * dir_j;
                }
            }
            let mut coeff1 = 0.0;
            for j in 0..solved_cols {
                coeff1 += scratch.work1[j] * scratch.work1[j];
            }
            let pc2 = previous_cost * previous_cost;
            coeff1 /= pc2;
            let coeff2 = lm_par * lm_norm * lm_norm / pc2;
            let pre_red = coeff1 + 2.0 * coeff2;
            let dir_der = -(coeff1 + coeff2);

            ratio = if pre_red == 0.0 {
                0.0
            } else {
                act_red / pre_red
            };

            // Update the step bound.
            if ratio <= 0.25 {
                let mut tmp = if act_red < 0.0 {
                    0.5 * dir_der / (dir_der + 0.5 * act_red)
                } else {
                    0.5
                };
                if (0.1 * current_cost >= previous_cost) || (tmp < 0.1) {
                    tmp = 0.1;
                }
                delta = tmp * delta.min(10.0 * lm_norm);
                lm_par /= tmp;
            } else if lm_par == 0.0 || ratio >= 0.75 {
                delta = 2.0 * lm_norm;
                lm_par *= 0.5;
            }

            if ratio >= 1.0e-4 {
                // Successful iteration: update the scaled point norm.
                first_iteration = false;
                x_norm = 0.0;
                for k in 0..n_c {
                    let xk = scratch.diag[k] * current_point[k];
                    x_norm += xk * xk;
                }
                x_norm = x_norm.sqrt();
                // Commons Math consults the problem's `ConvergenceChecker`
                // here. `LeastSquaresBuilder` was never given one, so it is
                // null and this branch is dead — noted rather than invented.
            } else {
                // Failed iteration: restore everything. Only the first
                // `solvedCols` permuted components come back, exactly as in
                // Java — see the note on `current_point`.
                for j in 0..solved_cols {
                    let pj = data.permutation[j];
                    current_point[pj] = scratch.old_x[pj];
                }
                std::mem::swap(&mut weighted_residual, &mut old_res);
                current = previous.clone();
            }

            // Default convergence criteria.
            if (act_red.abs() <= COST_RELATIVE_TOLERANCE
                && pre_red <= COST_RELATIVE_TOLERANCE
                && ratio <= 2.0)
                || delta <= PAR_RELATIVE_TOLERANCE * x_norm
            {
                return Ok(LmOptimum {
                    point: current.point,
                    iterations,
                });
            }

            // Termination on tolerances too stringent for this arithmetic.
            if act_red.abs() <= TWO_EPS && pre_red <= TWO_EPS && ratio <= 2.0 {
                return Err(FreesError::solver(
                    "Curve fit stalled: the cost relative tolerance (1e-10) is too small — \
                     no further reduction in the sum of squares is possible.",
                ));
            } else if delta <= TWO_EPS * x_norm {
                return Err(FreesError::solver(
                    "Curve fit stalled: the parameters relative tolerance (1e-10) is too small — \
                     no further improvement in the parameters is possible.",
                ));
            } else if max_cosine <= TWO_EPS {
                return Err(FreesError::solver(
                    "Curve fit stalled: the orthogonality tolerance (1e-10) is too small — \
                     the residual vector is already orthogonal to the model's Jacobian.",
                ));
            }
        }
    }
}

/// QR decomposition with column pivoting of the **negated** Jacobian. Port of
/// `LevenbergMarquardtOptimizer.qrDecomposition`.
fn qr_decomposition(
    jacobian: &Mat,
    solved_cols: usize,
    n_r: usize,
    n_c: usize,
) -> Result<InternalData> {
    let mut weighted_jacobian: Mat = jacobian
        .iter()
        .map(|row| row.iter().map(|v| -v).collect())
        .collect();

    let mut permutation: Vec<usize> = (0..n_c).collect();
    let mut diag_r = vec![0.0; n_c];
    let mut jac_norm = vec![0.0; n_c];
    let mut beta = vec![0.0; n_c];

    for k in 0..n_c {
        let mut norm2 = 0.0;
        for i in 0..n_r {
            let akk = weighted_jacobian[i][k];
            norm2 += akk * akk;
        }
        jac_norm[k] = norm2.sqrt();
    }

    for k in 0..n_c {
        // Pick the remaining column with the greatest norm on active rows.
        let mut next_column = usize::MAX;
        let mut ak2 = f64::NEG_INFINITY;
        for i in k..n_c {
            let mut norm2 = 0.0;
            for j in k..n_r {
                let aki = weighted_jacobian[j][permutation[i]];
                norm2 += aki * aki;
            }
            if !norm2.is_finite() {
                return Err(FreesError::solver(
                    "Curve fit failed: the model's Jacobian is not finite — check the \
                     model equation and the starting values.",
                ));
            }
            if norm2 > ak2 {
                next_column = i;
                ak2 = norm2;
            }
        }
        if ak2 <= SAFE_MIN {
            return Ok(InternalData {
                weighted_jacobian,
                permutation,
                rank: k,
                diag_r,
                jac_norm,
                beta,
            });
        }
        permutation.swap(next_column, k);
        let pk = permutation[k];

        // Choose alpha so that Hk·u = alpha·ek.
        let akk = weighted_jacobian[k][pk];
        let alpha = if akk > 0.0 { -ak2.sqrt() } else { ak2.sqrt() };
        let betak = 1.0 / (ak2 - akk * alpha);
        beta[pk] = betak;

        diag_r[pk] = alpha;
        weighted_jacobian[k][pk] -= alpha;

        // Apply the reflection to the remaining columns.
        for dk in (1..(n_c - k)).rev() {
            let pdk = permutation[k + dk];
            let mut gamma = 0.0;
            for j in k..n_r {
                gamma += weighted_jacobian[j][pk] * weighted_jacobian[j][pdk];
            }
            gamma *= betak;
            for j in k..n_r {
                let v = weighted_jacobian[j][pk];
                weighted_jacobian[j][pdk] -= gamma * v;
            }
        }
    }

    Ok(InternalData {
        weighted_jacobian,
        permutation,
        rank: solved_cols,
        diag_r,
        jac_norm,
        beta,
    })
}

/// `y ← Qᵀ·y` using the reflections stored in the decomposition. Port of
/// `LevenbergMarquardtOptimizer.qTy`.
fn q_t_y(y: &mut [f64], data: &InternalData, n_r: usize, n_c: usize) {
    for k in 0..n_c {
        let pk = data.permutation[k];
        let mut gamma = 0.0;
        for i in k..n_r {
            gamma += data.weighted_jacobian[i][pk] * y[i];
        }
        gamma *= data.beta[pk];
        for i in k..n_r {
            y[i] -= gamma * data.weighted_jacobian[i][pk];
        }
    }
}

/// The Levenberg–Marquardt parameter for this step, by MINPACK's `lmpar`
/// bisection-with-Newton on `‖D·p(λ)‖ − Δ`. Port of
/// `LevenbergMarquardtOptimizer.determineLMParameter`.
fn determine_lm_parameter(
    qy: &[f64],
    delta: f64,
    scratch: &mut LmScratch,
    data: &mut InternalData,
    solved_cols: usize,
    lm_par_in: f64,
) -> f64 {
    let mut lm_par = lm_par_in;
    let n_c = data.weighted_jacobian.first().map_or(0, Vec::len);
    let rank = data.rank;

    // The Gauss-Newton direction (least-squares one if R is rank-deficient).
    for j in 0..rank {
        scratch.lm_dir[data.permutation[j]] = qy[j];
    }
    for j in rank..n_c {
        scratch.lm_dir[data.permutation[j]] = 0.0;
    }
    for k in (0..rank).rev() {
        let pk = data.permutation[k];
        let ypk = scratch.lm_dir[pk] / data.diag_r[pk];
        for i in 0..k {
            scratch.lm_dir[data.permutation[i]] -= ypk * data.weighted_jacobian[i][pk];
        }
        scratch.lm_dir[pk] = ypk;
    }

    // Accept the Gauss-Newton step if it already sits inside the trust region.
    let mut dx_norm = 0.0;
    for j in 0..solved_cols {
        let pj = data.permutation[j];
        let s = scratch.diag[pj] * scratch.lm_dir[pj];
        scratch.work1[pj] = s;
        dx_norm += s * s;
    }
    let mut dx_norm = dx_norm.sqrt();
    let mut fp = dx_norm - delta;
    if fp <= 0.1 * delta {
        return 0.0;
    }

    // Lower bound `parl` — zero when R is rank-deficient.
    let mut parl = 0.0;
    if rank == solved_cols {
        for j in 0..solved_cols {
            let pj = data.permutation[j];
            scratch.work1[pj] *= scratch.diag[pj] / dx_norm;
        }
        let mut sum2 = 0.0;
        for j in 0..solved_cols {
            let pj = data.permutation[j];
            let mut sum = 0.0;
            for i in 0..j {
                sum += data.weighted_jacobian[i][pj] * scratch.work1[data.permutation[i]];
            }
            let s = (scratch.work1[pj] - sum) / data.diag_r[pj];
            scratch.work1[pj] = s;
            sum2 += s * s;
        }
        parl = fp / (delta * sum2);
    }

    // Upper bound `paru`.
    let mut sum2 = 0.0;
    for j in 0..solved_cols {
        let pj = data.permutation[j];
        let mut sum = 0.0;
        for i in 0..=j {
            sum += data.weighted_jacobian[i][pj] * qy[i];
        }
        sum /= scratch.diag[pj];
        sum2 += sum * sum;
    }
    let g_norm = sum2.sqrt();
    let mut paru = g_norm / delta;
    if paru == 0.0 {
        paru = SAFE_MIN / delta.min(0.1);
    }

    lm_par = paru.min(lm_par.max(parl));
    if lm_par == 0.0 {
        lm_par = g_norm / dx_norm;
    }

    for _ in 0..=10 {
        if lm_par == 0.0 {
            lm_par = SAFE_MIN.max(0.001 * paru);
        }
        let s_par = lm_par.sqrt();
        for j in 0..solved_cols {
            let pj = data.permutation[j];
            scratch.work1[pj] = s_par * scratch.diag[pj];
        }
        determine_lm_direction(qy, scratch, data, solved_cols);

        dx_norm = 0.0;
        for j in 0..solved_cols {
            let pj = data.permutation[j];
            let s = scratch.diag[pj] * scratch.lm_dir[pj];
            scratch.work3[pj] = s;
            dx_norm += s * s;
        }
        dx_norm = dx_norm.sqrt();
        let previous_fp = fp;
        fp = dx_norm - delta;

        // Close enough, or the exceptional `parl == 0` case.
        if fp.abs() <= 0.1 * delta || (parl == 0.0 && fp <= previous_fp && previous_fp < 0.0) {
            return lm_par;
        }

        // Newton correction.
        for j in 0..solved_cols {
            let pj = data.permutation[j];
            scratch.work1[pj] = scratch.work3[pj] * scratch.diag[pj] / dx_norm;
        }
        for j in 0..solved_cols {
            let pj = data.permutation[j];
            scratch.work1[pj] /= scratch.work2[j];
            let tmp = scratch.work1[pj];
            for i in (j + 1)..solved_cols {
                scratch.work1[data.permutation[i]] -= data.weighted_jacobian[i][pj] * tmp;
            }
        }
        let mut sum2 = 0.0;
        for j in 0..solved_cols {
            let s = scratch.work1[data.permutation[j]];
            sum2 += s * s;
        }
        let correction = fp / (delta * sum2);

        if fp > 0.0 {
            parl = parl.max(lm_par);
        } else if fp < 0.0 {
            paru = paru.min(lm_par);
        }
        lm_par = parl.max(lm_par + correction);
    }

    lm_par
}

/// Solves `(RᵀR + DᵀD)·x = Rᵀ·Qᵀ·y` by Givens elimination of `D`, MINPACK's
/// `qrsolv`. Port of `LevenbergMarquardtOptimizer.determineLMDirection`.
///
/// `scratch.work1` is the scaled diagonal `D`, `scratch.work2` receives the
/// diagonal of `S`, and `scratch.work3` is the work vector. The strictly-lower
/// triangle of `weighted_jacobian` is left holding `S`, which
/// [`determine_lm_parameter`] then reads for its Newton correction — that
/// aliasing is the reference's, kept deliberately.
fn determine_lm_direction(
    qy: &[f64],
    scratch: &mut LmScratch,
    data: &mut InternalData,
    solved_cols: usize,
) {
    // Copy R and Qᵀy, saving R's diagonal in `lm_dir`.
    for j in 0..solved_cols {
        let pj = data.permutation[j];
        for i in (j + 1)..solved_cols {
            data.weighted_jacobian[i][pj] = data.weighted_jacobian[j][data.permutation[i]];
        }
        scratch.lm_dir[j] = data.diag_r[pj];
        scratch.work3[j] = qy[j];
    }

    // Eliminate the diagonal matrix D with Givens rotations.
    for j in 0..solved_cols {
        let pj = data.permutation[j];
        let dpj = scratch.work1[pj];
        if dpj != 0.0 {
            // The Java fills to `lmDiag.length` — the full nC, not solvedCols.
            for slot in scratch.work2.iter_mut().skip(j + 1) {
                *slot = 0.0;
            }
        }
        scratch.work2[j] = dpj;

        let mut qtbpj = 0.0;
        for k in j..solved_cols {
            let pk = data.permutation[k];
            if scratch.work2[k] != 0.0 {
                let rkk = data.weighted_jacobian[k][pk];
                let (sin, cos) = if rkk.abs() < scratch.work2[k].abs() {
                    let cotan = rkk / scratch.work2[k];
                    let sin = 1.0 / (1.0 + cotan * cotan).sqrt();
                    (sin, sin * cotan)
                } else {
                    let tan = scratch.work2[k] / rkk;
                    let cos = 1.0 / (1.0 + tan * tan).sqrt();
                    (cos * tan, cos)
                };

                data.weighted_jacobian[k][pk] = cos * rkk + sin * scratch.work2[k];
                let temp = cos * scratch.work3[k] + sin * qtbpj;
                qtbpj = -sin * scratch.work3[k] + cos * qtbpj;
                scratch.work3[k] = temp;

                for i in (k + 1)..solved_cols {
                    let rik = data.weighted_jacobian[i][pk];
                    let temp2 = cos * rik + sin * scratch.work2[i];
                    scratch.work2[i] = -sin * rik + cos * scratch.work2[i];
                    data.weighted_jacobian[i][pk] = temp2;
                }
            }
        }

        // Store S's diagonal element and restore R's.
        scratch.work2[j] = data.weighted_jacobian[j][data.permutation[j]];
        data.weighted_jacobian[j][data.permutation[j]] = scratch.lm_dir[j];
    }

    // Back-substitute; a singular system yields a least-squares solution.
    let mut n_sing = solved_cols;
    for j in 0..solved_cols {
        if scratch.work2[j] == 0.0 && n_sing == solved_cols {
            n_sing = j;
        }
        if n_sing < solved_cols {
            scratch.work3[j] = 0.0;
        }
    }
    if n_sing > 0 {
        for j in (0..n_sing).rev() {
            let pj = data.permutation[j];
            let mut sum = 0.0;
            for i in (j + 1)..n_sing {
                sum += data.weighted_jacobian[i][pj] * scratch.work3[i];
            }
            scratch.work3[j] = (scratch.work3[j] - sum) / scratch.work2[j];
        }
    }

    for j in 0..scratch.lm_dir.len() {
        scratch.lm_dir[data.permutation[j]] = scratch.work3[j];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every expectation below is the Java `CurveFitter`'s own output, captured
    /// by running the reference engine off `tools/golden-dumper/classpath.sh`
    /// with the identical model, data and start. `iterations` is quoted too —
    /// it is the value that would drift first if the damping schedule were
    /// paraphrased rather than transcribed.
    fn close(actual: f64, expected: f64, tol: f64) {
        assert!(
            (actual - expected).abs() <= tol * expected.abs().max(1.0),
            "expected {expected}, got {actual} (tol {tol})"
        );
    }

    fn names(list: &[String]) -> Vec<&str> {
        list.iter().map(String::as_str).collect()
    }

    fn fit_of(
        model: &str,
        params: &[&str],
        x: &[f64],
        y: &[f64],
        start: Option<&[f64]>,
    ) -> FitResult {
        let params: Vec<String> = params.iter().map(|s| (*s).to_string()).collect();
        fit(model, "y", "x", &params, x, y, start).expect("fit")
    }

    #[test]
    fn oracle_exponential_decay_from_the_default_start() {
        // Java: params [4.999999999999999, 0.7000000000000001, 1.5000000000000002]
        //       r2 1.0, rmse 3.6259732146947156E-16, iters 9
        let x: [f64; 9] = [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0];
        let y: Vec<f64> = x.iter().map(|x| 5.0 * (-0.7 * x).exp() + 1.5).collect();
        let r = fit_of("y = a * exp(-b * x) + c", &["a", "b", "c"], &x, &y, None);
        assert_eq!(names(&r.parameter_names), ["a", "b", "c"]);
        close(r.fitted_parameters[0], 4.999999999999999, 1e-9);
        close(r.fitted_parameters[1], 0.7000000000000001, 1e-9);
        close(r.fitted_parameters[2], 1.5000000000000002, 1e-9);
        close(r.r_squared, 1.0, 1e-12);
        assert!(r.rmse < 1e-12, "rmse {} should be ~0", r.rmse);
        assert_eq!(r.iterations, 9);
    }

    #[test]
    fn oracle_exponential_decay_with_scatter_and_a_custom_start() {
        // Java: params [5.007706988613813, 0.690465839847928, 1.4747194620489636]
        //       r2 0.9994820045657309, rmse 0.034237712312263545, iters 5
        let x: [f64; 9] = [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0];
        let y: Vec<f64> = x
            .iter()
            .map(|x| 5.0 * (-0.7 * x).exp() + 1.5 + 0.05 * (9.0 * x).sin())
            .collect();
        let r = fit_of(
            "y = a * exp(-b * x) + c",
            &["a", "b", "c"],
            &x,
            &y,
            Some(&[4.0, 1.0, 1.0]),
        );
        close(r.fitted_parameters[0], 5.007706988613813, 1e-8);
        close(r.fitted_parameters[1], 0.690465839847928, 1e-8);
        close(r.fitted_parameters[2], 1.4747194620489636, 1e-8);
        close(r.r_squared, 0.9994820045657309, 1e-10);
        close(r.rmse, 0.034237712312263545, 1e-9);
        assert_eq!(r.iterations, 5);
        // Residuals are `observed - fitted`, in data order.
        close(r.residuals[0], 0.017573549337223326, 1e-7);
        close(r.residuals[8], -0.036614686745172875, 1e-7);
        close(r.fitted_values[0], 6.482426450662777, 1e-9);
    }

    #[test]
    fn oracle_linear_model() {
        // Java: params [3.0, -2.0], r2 1.0, rmse 0.0, iters 3
        let x: [f64; 5] = [1.0, 2.0, 3.0, 4.0, 5.0];
        let y: Vec<f64> = x.iter().map(|x| 3.0 * x - 2.0).collect();
        let r = fit_of("y = m * x + q", &["m", "q"], &x, &y, None);
        close(r.fitted_parameters[0], 3.0, 1e-10);
        close(r.fitted_parameters[1], -2.0, 1e-10);
        assert_eq!(r.iterations, 3);
        assert!(r.rmse < 1e-12);
    }

    #[test]
    fn oracle_power_law_from_a_bad_start() {
        // Java: params [2.499999999999999, 1.8000000000000003], r2 1.0, iters 7
        let x: [f64; 6] = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let y: Vec<f64> = x.iter().map(|x| 2.5 * x.powf(1.8)).collect();
        let r = fit_of("y = k * x^n", &["k", "n"], &x, &y, Some(&[1.0, 1.0]));
        close(r.fitted_parameters[0], 2.499999999999999, 1e-9);
        close(r.fitted_parameters[1], 1.8000000000000003, 1e-9);
        assert_eq!(r.iterations, 7);
    }

    #[test]
    fn oracle_overdetermined_quadratic_with_scatter() {
        // Java: params [0.5003377168298639, -1.2596151418030361, 3.0045971425232336]
        //       r2 0.999415980745421, rmse 0.06957503490214675, iters 2
        let x: [f64; 8] = [-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0, 4.0];
        let y: Vec<f64> = x
            .iter()
            .map(|x| 0.5 * x * x - 1.25 * x + 3.0 + 0.1 * (4.0 * x).cos())
            .collect();
        let r = fit_of(
            "y = a2 * x^2 + a1 * x + a0",
            &["a2", "a1", "a0"],
            &x,
            &y,
            None,
        );
        close(r.fitted_parameters[0], 0.5003377168298639, 1e-9);
        close(r.fitted_parameters[1], -1.2596151418030361, 1e-9);
        close(r.fitted_parameters[2], 3.0045971425232336, 1e-9);
        close(r.r_squared, 0.999415980745421, 1e-10);
        close(r.rmse, 0.06957503490214675, 1e-9);
        assert_eq!(r.iterations, 2);
    }

    #[test]
    fn oracle_single_parameter() {
        // Java: params [7.0], r2 1.0, rmse 0.0, iters 3
        let x: [f64; 3] = [1.0, 2.0, 3.0];
        let y: [f64; 3] = [7.0, 14.0, 21.0];
        let r = fit_of("y = a * x", &["a"], &x, &y, None);
        close(r.fitted_parameters[0], 7.0, 1e-10);
        assert_eq!(r.iterations, 3);
        assert!(r.rmse < 1e-12);
    }

    // -- the validation half ---------------------------------------------

    #[test]
    fn a_blank_model_is_refused() {
        let err = fit("  ", "y", "x", &["a".into()], &[1.0], &[1.0], None).unwrap_err();
        assert_eq!(err.to_string_message(), "Model equation is required.");
    }

    #[test]
    fn mismatched_data_lengths_are_refused() {
        let err = fit(
            "y = a * x",
            "y",
            "x",
            &["a".into()],
            &[1.0, 2.0],
            &[1.0],
            None,
        )
        .unwrap_err();
        assert_eq!(
            err.to_string_message(),
            "x and y data must have the same length (got 2 and 1)."
        );
    }

    #[test]
    fn empty_data_and_empty_parameters_are_refused() {
        let err = fit("y = a * x", "y", "x", &["a".into()], &[], &[], None).unwrap_err();
        assert_eq!(err.to_string_message(), "Data points are required.");
        let err = fit("y = a * x", "y", "x", &[], &[1.0], &[1.0], None).unwrap_err();
        assert_eq!(
            err.to_string_message(),
            "At least one parameter to fit is required."
        );
    }

    #[test]
    fn a_model_that_does_not_isolate_the_dependent_variable_is_refused() {
        let err = fit(
            "y + z = a * x",
            "y",
            "x",
            &["a".into()],
            &[1.0],
            &[1.0],
            None,
        )
        .unwrap_err();
        assert!(
            err.to_string_message()
                .starts_with("Could not identify 'y' as the dependent variable"),
            "{}",
            err.to_string_message()
        );
    }

    #[test]
    fn the_model_may_be_written_with_the_dependent_variable_on_the_right() {
        // `parseModelExpression` accepts `<expr> = y` too.
        let x: [f64; 3] = [1.0, 2.0, 3.0];
        let y: [f64; 3] = [7.0, 14.0, 21.0];
        let r = fit_of("a * x = y", &["a"], &x, &y, None);
        close(r.fitted_parameters[0], 7.0, 1e-10);
    }

    #[test]
    fn a_short_initial_guess_defaults_the_rest_to_one() {
        // Only `a` is seeded; `b` and `c` start at 1.0 like the Java's
        // per-index `i < initialGuess.size()` test.
        let x: [f64; 9] = [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0];
        let y: Vec<f64> = x.iter().map(|x| 5.0 * (-0.7 * x).exp() + 1.5).collect();
        let seeded = fit_of(
            "y = a * exp(-b * x) + c",
            &["a", "b", "c"],
            &x,
            &y,
            Some(&[1.0]),
        );
        let defaulted = fit_of("y = a * exp(-b * x) + c", &["a", "b", "c"], &x, &y, None);
        assert_eq!(seeded.iterations, defaulted.iterations);
        close(
            seeded.fitted_parameters[1],
            defaulted.fitted_parameters[1],
            1e-12,
        );
    }

    #[test]
    fn parameter_names_are_reported_lowercase() {
        let x: [f64; 3] = [1.0, 2.0, 3.0];
        let y: [f64; 3] = [7.0, 14.0, 21.0];
        let params = ["Slope".to_string()];
        let r = fit("y = Slope * x", "Y", "X", &params, &x, &y, None).expect("fit");
        assert_eq!(names(&r.parameter_names), ["slope"]);
        close(r.fitted_parameters[0], 7.0, 1e-10);
    }

    #[test]
    fn r_squared_is_one_when_the_observations_have_no_spread() {
        // ssTot == 0 → the Java returns 1.0 rather than dividing by zero.
        let x: [f64; 3] = [1.0, 2.0, 3.0];
        let y: [f64; 3] = [4.0, 4.0, 4.0];
        let r = fit_of("y = c + 0 * x", &["c"], &x, &y, None);
        assert_eq!(r.r_squared, 1.0);
    }
}
