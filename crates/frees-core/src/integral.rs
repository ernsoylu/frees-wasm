//! Equation-based integral support (calculus).
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/core/IntegralSolver.java`
//! (439 LOC) plus the two quadrature arms of
//! `../frEES/backend/core/src/main/java/com/frees/backend/ast/Evaluator.java`
//! that `Integral(...)` / `GaussIntegral(...)` dispatch to when they are
//! evaluated *inside an expression* (`integralQuadrature`,
//! `gaussLegendreQuadrature`).
//!
//! # Two different integrators, on purpose
//!
//! `Integral(f, t, a, b[, step])` has **two** execution paths in the parent
//! engine, and they do not share an algorithm:
//!
//! * **Constant limits** — the engine drives `t` from `a` to `b` itself,
//!   re-solving the rest of the system at every step, and accumulates with a
//!   second-order predictor–corrector ([`integrate`], Java
//!   `IntegralSolver.integrate`). The driver lives in [`crate::engine`]; this
//!   module owns the stepper. Because `t` is driven, it **survives as a result
//!   variable pinned at the upper limit** — that is why
//!   `F = Integral(t^2, t, 0, 1)` reports both `F` and `t = 1`, and why the
//!   value (`0.33333333600411386`) carries the stepper's truncation error
//!   rather than being exactly `1/3`.
//! * **Variable limits** (`F = Integral(2*t, t, 0, b)`) — the limit is an
//!   unknown, so stepping is impossible. [`inlined_equation`] rewrites the
//!   integral into an ordinary equation whose integrand is closed-form in `t`,
//!   and the evaluator computes it by **adaptive Simpson** ([`integral`]) at
//!   every Newton residual evaluation.
//!
//! `GaussIntegral(f, t, a, b[, points])` is always the second kind: a bound
//! `t`, no system coupling, evaluated in place by the iterative
//! Gauss–Legendre rule Apache Commons Math implements as
//! `IterativeLegendreGaussIntegrator(points, 1e-10, 1e-10, 2, 64)`
//! ([`gauss_integral`]). It is exact for `∫₀¹ t² dt` where the stepper is not.
//!
//! # Structural view
//!
//! An integral equation is not a residual the blocker can match: `t` appears
//! in it but is determined by the integration, not by the equation. So before
//! blocking, [`structural_view`] replaces every integral equation with a
//! placeholder (constant limits) or its inlined quadrature form (variable
//! limits) and, **once per distinct integration variable**, adds a pin
//! `t = <upper>`. Without that pin `F = Integral(t^2, t, 0, 1)` is one
//! equation in two unknowns and the blocker rejects it.
//!
//! # No clock
//!
//! The Java stepper takes a `deadlineNanos` and aborts on it.
//! `wasm32-unknown-unknown` has no clock (see [`crate::engine::SolveStats`]),
//! so the step budget [`MAX_STEPS`] is the only bound, exactly as
//! `try_univariate_bracketing_solve` uses its sample count instead of the
//! Java elapsed-time check.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::ast::{Equation, Expr};
use crate::diag::{FreesError, Result};
use crate::eval::{eval_with, EvalContext, Scope};
use crate::parser::defs::Definitions;

/// The lowercase call name the parser produces for `Integral(...)`.
/// `IntegralSolver.FUNCTION_NAME`.
pub const FUNCTION_NAME: &str = "integral";

/// Step-doubling/halving tolerance on the predictor–corrector difference.
const REL_TOL: f64 = 1.0e-6;
/// Floor for the relative error scale, so a near-zero running total does not
/// force the step size to collapse.
const ABS_FLOOR: f64 = 1.0e-10;
/// Initial step count of the adaptive sweep (`h = (b - a) / 20`).
const INITIAL_STEPS: usize = 20;
/// Hard cap on accepted+rejected steps of one sweep.
pub const MAX_STEPS: usize = 200_000;

// ---------------------------------------------------------------------------
// The extracted integral equation
// ---------------------------------------------------------------------------

/// One `F = Integral(f, t, a, b[, step])` equation, decomposed.
///
/// Port of the Java record `IntegralSolver.IntegralEquation`.
#[derive(Debug, Clone, PartialEq)]
pub struct IntegralEquation {
    /// The equation exactly as it appeared, used to quote diagnostics and to
    /// identify the equation for removal in [`ordinary_equations`].
    pub original: Equation,
    /// The variable the integral value is assigned to (lowercase).
    pub result_var: String,
    /// The integrand expression (argument 0).
    pub integrand: Expr,
    /// The integration variable (argument 1, lowercase).
    pub integration_var: String,
    /// Lower limit as written (argument 2).
    pub lower_expr: Expr,
    /// Upper limit as written (argument 3).
    pub upper_expr: Expr,
    /// The lower limit if it evaluates against an *empty* scope, else `None`.
    pub lower_const: Option<f64>,
    /// The upper limit if it evaluates against an *empty* scope, else `None`.
    pub upper_const: Option<f64>,
    /// Forced step size (argument 4), `0.0` when absent — a positive value
    /// disables step adaptation in [`integrate`].
    pub fixed_step: f64,
}

impl IntegralEquation {
    /// Constant limits use the stepping driver; variable limits (an unknown
    /// like `T_flame`) are inlined into the equation system.
    pub fn constant_limits(&self) -> bool {
        self.lower_const.is_some() && self.upper_const.is_some()
    }

    /// The constant lower limit. `NaN` when the limit is not constant — the
    /// Java accessor unboxes a `null` and throws; callers must gate on
    /// [`IntegralEquation::constant_limits`] either way.
    pub fn lower(&self) -> f64 {
        self.lower_const.unwrap_or(f64::NAN)
    }

    /// The constant upper limit. See [`IntegralEquation::lower`].
    pub fn upper(&self) -> f64 {
        self.upper_const.unwrap_or(f64::NAN)
    }
}

// ---------------------------------------------------------------------------
// Hoisting nested Integral calls
// ---------------------------------------------------------------------------

/// `Integral` may appear inside an arbitrary expression
/// (`y = y0 + Integral(f, t, a, b)`). The solver drives integrals only in the
/// *alone* form `F = Integral(...)`, so every nested call is hoisted into a
/// synthetic result variable (`integral_1`, `integral_2`, …) with its own
/// defining equation; the rewritten system is otherwise unchanged.
///
/// A document that mentions no integral is returned untouched — that
/// short-circuit is what keeps integral-free documents byte-identical through
/// the pass.
///
/// Port of `IntegralSolver.hoistNested`.
pub fn hoist_nested(equations: Vec<Equation>) -> Vec<Equation> {
    let mentions = equations
        .iter()
        .any(|eq| mentions_integral(&eq.lhs) || mentions_integral(&eq.rhs));
    if !mentions {
        return equations;
    }
    let mut taken: HashSet<String> = HashSet::new();
    for eq in &equations {
        taken.extend(eq.variables());
    }
    let mut rewritten: Vec<Equation> = Vec::with_capacity(equations.len());
    let mut hoisted: Vec<Equation> = Vec::new();
    for eq in equations {
        if alone_form(&eq) {
            rewritten.push(eq);
            continue;
        }
        let lhs = hoist_calls(&eq.lhs, &eq.source_text, &mut taken, &mut hoisted);
        let rhs = hoist_calls(&eq.rhs, &eq.source_text, &mut taken, &mut hoisted);
        rewritten.push(Equation::new(lhs, rhs, eq.source_text));
    }
    rewritten.extend(hoisted);
    rewritten
}

/// `IntegralSolver.aloneForm`: a bare variable on one side, the `Integral`
/// call on the other.
fn alone_form(eq: &Equation) -> bool {
    (matches!(eq.lhs, Expr::Var(_)) && is_integral_call(&eq.rhs))
        || (matches!(eq.rhs, Expr::Var(_)) && is_integral_call(&eq.lhs))
}

fn is_integral_call(e: &Expr) -> bool {
    matches!(e, Expr::Call { function, .. } if function == FUNCTION_NAME)
}

/// `IntegralSolver.hoistCalls`. Integral calls inside other node types (array
/// indices, ranges) are left in place; [`extract`] rejects them with its usual
/// guidance.
fn hoist_calls(
    e: &Expr,
    source_text: &str,
    taken: &mut HashSet<String>,
    hoisted: &mut Vec<Equation>,
) -> Expr {
    match e {
        Expr::Neg(operand) => {
            Expr::Neg(Box::new(hoist_calls(operand, source_text, taken, hoisted)))
        }
        Expr::BinOp { op, left, right } => Expr::BinOp {
            op: *op,
            left: Box::new(hoist_calls(left, source_text, taken, hoisted)),
            right: Box::new(hoist_calls(right, source_text, taken, hoisted)),
        },
        Expr::Call { function, args } => {
            if function == FUNCTION_NAME {
                let name = fresh_name(taken);
                hoisted.push(Equation::new(
                    Expr::Var(name.clone()),
                    e.clone(),
                    source_text.to_string(),
                ));
                return Expr::Var(name);
            }
            Expr::Call {
                function: function.clone(),
                args: args
                    .iter()
                    .map(|a| hoist_calls(a, source_text, taken, hoisted))
                    .collect(),
            }
        }
        other => other.clone(),
    }
}

/// `IntegralSolver.freshName`: the first `integral_<n>` not already used as a
/// variable, claimed as it is handed out.
fn fresh_name(taken: &mut HashSet<String>) -> String {
    let mut n = 1usize;
    let mut name = format!("integral_{n}");
    while !taken.insert(name.clone()) {
        n += 1;
        name = format!("integral_{n}");
    }
    name
}

// ---------------------------------------------------------------------------
// Extraction
// ---------------------------------------------------------------------------

/// True when `e` contains an `Integral` call anywhere.
/// Port of `IntegralSolver.mentionsIntegral` (every `Expr` variant enumerated,
/// as the Java exhaustive switch does).
pub fn mentions_integral(e: &Expr) -> bool {
    match e {
        Expr::Num { .. } | Expr::Str(_) | Expr::Var(_) => false,
        Expr::Neg(operand) | Expr::Not(operand) => mentions_integral(operand),
        Expr::BinOp { left, right, .. }
        | Expr::Compare { left, right, .. }
        | Expr::Logical { left, right, .. }
        | Expr::Range {
            start: left,
            end: right,
        } => mentions_integral(left) || mentions_integral(right),
        Expr::Call { function, args } => {
            function == FUNCTION_NAME || args.iter().any(mentions_integral)
        }
        // The Java arm for ArrayAccess inspects only the indices — the array
        // name cannot carry a call.
        Expr::ArrayAccess { indices, .. } => indices.iter().any(mentions_integral),
        Expr::ArrayLiteral(elements) => elements.iter().any(mentions_integral),
    }
}

/// Every `Integral` equation of the system.
///
/// # Errors
///
/// [`FreesError::Solver`] when `Integral` is used anywhere except alone on one
/// side of an equation, or when its argument list is malformed
/// (`IntegralSolver.extract` + `toIntegralEquation`).
pub fn extract(equations: &[Equation], defs: &Definitions) -> Result<Vec<IntegralEquation>> {
    let mut integrals = Vec::new();
    for eq in equations {
        match match_integral_equation(eq, defs)? {
            Some(ie) => integrals.push(ie),
            None if mentions_integral(&eq.lhs) || mentions_integral(&eq.rhs) => {
                return Err(FreesError::solver(format!(
                    "Integral must appear alone on one side of an equation: \
                     F = Integral(f, t, a, b). Offending equation: {}",
                    eq.source_text
                )));
            }
            None => {}
        }
    }
    Ok(integrals)
}

/// `IntegralSolver.matchIntegralEquation`. (The Java also asserts the *bare*
/// side does not mention an integral; a `Var` never can, so that guard is
/// dropped rather than transcribed as a constant `false`.)
fn match_integral_equation(eq: &Equation, defs: &Definitions) -> Result<Option<IntegralEquation>> {
    if let (Expr::Var(name), Expr::Call { function, args }) = (&eq.lhs, &eq.rhs) {
        if function == FUNCTION_NAME {
            return to_integral_equation(eq, name, args, defs).map(Some);
        }
    }
    if let (Expr::Call { function, args }, Expr::Var(name)) = (&eq.lhs, &eq.rhs) {
        if function == FUNCTION_NAME {
            return to_integral_equation(eq, name, args, defs).map(Some);
        }
    }
    Ok(None)
}

/// `IntegralSolver.toIntegralEquation`.
fn to_integral_equation(
    eq: &Equation,
    result_var: &str,
    args: &[Expr],
    defs: &Definitions,
) -> Result<IntegralEquation> {
    if args.len() < 4 || args.len() > 5 {
        return Err(FreesError::solver(format!(
            "Integral expects Integral(f, t, lower, upper[, step]): {}",
            eq.source_text
        )));
    }
    let Expr::Var(t_name) = &args[1] else {
        return Err(FreesError::solver(format!(
            "The second argument of Integral must be the integration variable: {}",
            eq.source_text
        )));
    };
    let fixed_step = match args.get(4) {
        Some(step) => constant_arg(step, "step size", eq, defs)?,
        None => 0.0,
    };
    Ok(IntegralEquation {
        original: eq.clone(),
        result_var: result_var.to_string(),
        integrand: args[0].clone(),
        integration_var: t_name.clone(),
        lower_expr: args[2].clone(),
        upper_expr: args[3].clone(),
        lower_const: try_constant(&args[2], defs),
        upper_const: try_constant(&args[3], defs),
        fixed_step,
    })
}

/// The limit's value if it is closed (evaluates against an empty scope), else
/// `None` — it contains unknowns and is resolved by the equation system.
/// Java catches the evaluator's `IllegalStateException` for exactly this.
fn try_constant(e: &Expr, defs: &Definitions) -> Option<f64> {
    eval_with(e, &Scope::new(), EvalContext::with_defs(defs)).ok()
}

/// `IntegralSolver.constantArg`: an argument that *must* be closed.
fn constant_arg(e: &Expr, what: &str, eq: &Equation, defs: &Definitions) -> Result<f64> {
    eval_with(e, &Scope::new(), EvalContext::with_defs(defs)).map_err(|_| {
        FreesError::solver(format!(
            "The {what} of Integral must be a numeric constant: {}",
            eq.source_text
        ))
    })
}

// ---------------------------------------------------------------------------
// The structural view the blocker sees
// ---------------------------------------------------------------------------

/// The system without its `Integral` equations.
/// Port of `IntegralSolver.ordinaryEquations`.
pub fn ordinary_equations(equations: &[Equation], integrals: &[IntegralEquation]) -> Vec<Equation> {
    equations
        .iter()
        .filter(|eq| !integrals.iter().any(|ie| ie.original == **eq))
        .cloned()
        .collect()
}

/// Equations equivalent to the originals **for structure checking only**:
/// each constant-limit `Integral` pins its result variable (it is driven
/// internally) while a variable-limit `Integral` contributes its actual
/// inlined equation; each integration variable gets a synthetic defining
/// equation so the system is square.
///
/// Port of `IntegralSolver.structuralView`.
pub fn structural_view(
    equations: &[Equation],
    integrals: &[IntegralEquation],
) -> Result<Vec<Equation>> {
    let ordinary = ordinary_equations(equations, integrals);
    let mut view = ordinary.clone();
    let mut pinned_integration_vars: BTreeSet<String> = BTreeSet::new();
    for ie in integrals {
        if ie.constant_limits() {
            view.push(Equation::new(
                Expr::Var(ie.result_var.clone()),
                Expr::num(0.0),
                ie.original.source_text.clone(),
            ));
        } else {
            view.push(inlined_equation(ie, &ordinary)?);
        }
        if pinned_integration_vars.insert(ie.integration_var.clone()) {
            let pin = if ie.constant_limits() {
                Expr::num(0.0)
            } else {
                ie.upper_expr.clone()
            };
            view.push(Equation::new(
                Expr::Var(ie.integration_var.clone()),
                pin,
                format!("{} (integration variable)", ie.integration_var),
            ));
        }
    }
    Ok(view)
}

/// An `Integral` with a variable limit (*find `T_flame` such that the energy
/// balance closes*) cannot be driven by stepping: the limit is unknown until
/// the system is solved. Instead the integral becomes an ordinary equation,
/// `result = Integral(f, t, a, b)`, that the evaluator computes by quadrature
/// at every Newton residual evaluation. For that the integrand must be
/// closed-form in the integration variable, so every variable in it that
/// (transitively) depends on `t` is replaced by its explicit definition:
/// `Cp_co2 = A + B*T + …` gets substituted into `Integral(Cp_co2, T, …)`.
///
/// Port of `IntegralSolver.inlinedEquation`.
///
/// # Errors
///
/// [`FreesError::Solver`] when the integrand references the integral's own
/// result variable, when a `t`-dependent variable has no unambiguous explicit
/// definition (including a circular definition chain), or when the integrand
/// contains a construct that cannot be inlined (a string, an array access, a
/// range).
pub fn inlined_equation(ie: &IntegralEquation, ordinary: &[Equation]) -> Result<Equation> {
    if ie.integrand.variables().contains(&ie.result_var) {
        return Err(FreesError::solver(format!(
            "An Integral with variable limits cannot reference its own result: {}",
            ie.original.source_text
        )));
    }
    let definitions = explicit_definitions(ordinary);
    let mut depends_memo: HashMap<String, bool> = HashMap::new();
    let mut expanding: HashSet<String> = HashSet::new();
    let inlined = inline(
        &ie.integrand,
        ie,
        &definitions,
        &mut depends_memo,
        &mut expanding,
    )?;
    let call = Expr::Call {
        function: FUNCTION_NAME.to_string(),
        args: vec![
            inlined,
            Expr::Var(ie.integration_var.clone()),
            ie.lower_expr.clone(),
            ie.upper_expr.clone(),
        ],
    };
    Ok(Equation::new(
        Expr::Var(ie.result_var.clone()),
        call,
        ie.original.source_text.clone(),
    ))
}

/// Unambiguous explicit definitions: equations of the form `v = expr` (or
/// `expr = v`) where `expr` does not contain `v` and `v` is defined once.
/// Port of `IntegralSolver.explicitDefinitions`.
fn explicit_definitions(equations: &[Equation]) -> HashMap<String, Expr> {
    let mut definitions: HashMap<String, Expr> = HashMap::new();
    let mut ambiguous: HashSet<String> = HashSet::new();
    for eq in equations {
        let mut named: Option<(&String, &Expr)> = None;
        if let Expr::Var(lhs_name) = &eq.lhs {
            if !eq.rhs.variables().contains(lhs_name) {
                named = Some((lhs_name, &eq.rhs));
            }
        }
        if named.is_none() {
            if let Expr::Var(rhs_name) = &eq.rhs {
                if !eq.lhs.variables().contains(rhs_name) {
                    named = Some((rhs_name, &eq.lhs));
                }
            }
        }
        if let Some((name, expr)) = named {
            // Java `putIfAbsent(...) != null` — a second definition makes the
            // name ambiguous and it is dropped entirely below.
            if definitions.contains_key(name) {
                ambiguous.insert(name.clone());
            } else {
                definitions.insert(name.clone(), expr.clone());
            }
        }
    }
    for name in &ambiguous {
        definitions.remove(name);
    }
    definitions
}

/// `IntegralSolver.dependsOnIntegrationVar`, memoised, with a `visiting` set so
/// a circular definition chain terminates (it "resolves through its other
/// members", and then fails in [`inline`] with the actionable message).
fn depends_on_integration_var(
    var_name: &str,
    ie: &IntegralEquation,
    definitions: &HashMap<String, Expr>,
    memo: &mut HashMap<String, bool>,
    visiting: &mut HashSet<String>,
) -> bool {
    if var_name == ie.integration_var {
        return true;
    }
    if let Some(&known) = memo.get(var_name) {
        return known;
    }
    if !visiting.insert(var_name.to_string()) {
        return false;
    }
    let mut depends = false;
    if let Some(definition) = definitions.get(var_name) {
        for inner in definition.variables() {
            if depends_on_integration_var(&inner, ie, definitions, memo, visiting) {
                depends = true;
                break;
            }
        }
    }
    visiting.remove(var_name);
    memo.insert(var_name.to_string(), depends);
    depends
}

/// `IntegralSolver.inline`.
fn inline(
    e: &Expr,
    ie: &IntegralEquation,
    definitions: &HashMap<String, Expr>,
    memo: &mut HashMap<String, bool>,
    expanding: &mut HashSet<String>,
) -> Result<Expr> {
    match e {
        Expr::Num { .. } => Ok(e.clone()),
        Expr::Var(name) => {
            // The Java calls dependsOnIntegrationVar with a *fresh* visiting
            // set at each inline site; only the memo is shared.
            let mut visiting = HashSet::new();
            if name == &ie.integration_var
                || !depends_on_integration_var(name, ie, definitions, memo, &mut visiting)
            {
                return Ok(Expr::Var(name.clone()));
            }
            match definitions.get(name) {
                Some(definition) if expanding.insert(name.clone()) => {
                    let inlined = inline(definition, ie, definitions, memo, expanding)?;
                    expanding.remove(name);
                    Ok(inlined)
                }
                _ => Err(FreesError::solver(format!(
                    "In {}: '{name}' depends on the integration variable {} \
                     but has no explicit definition of the form {name} = expression.",
                    ie.original.source_text, ie.integration_var
                ))),
            }
        }
        Expr::Neg(operand) => Ok(Expr::Neg(Box::new(inline(
            operand,
            ie,
            definitions,
            memo,
            expanding,
        )?))),
        Expr::Not(operand) => Ok(Expr::Not(Box::new(inline(
            operand,
            ie,
            definitions,
            memo,
            expanding,
        )?))),
        Expr::BinOp { op, left, right } => Ok(Expr::BinOp {
            op: *op,
            left: Box::new(inline(left, ie, definitions, memo, expanding)?),
            right: Box::new(inline(right, ie, definitions, memo, expanding)?),
        }),
        Expr::Compare { op, left, right } => Ok(Expr::Compare {
            op: *op,
            left: Box::new(inline(left, ie, definitions, memo, expanding)?),
            right: Box::new(inline(right, ie, definitions, memo, expanding)?),
        }),
        Expr::Logical { op, left, right } => Ok(Expr::Logical {
            op: *op,
            left: Box::new(inline(left, ie, definitions, memo, expanding)?),
            right: Box::new(inline(right, ie, definitions, memo, expanding)?),
        }),
        Expr::Call { function, args } => {
            let mut inlined = Vec::with_capacity(args.len());
            for arg in args {
                inlined.push(inline(arg, ie, definitions, memo, expanding)?);
            }
            Ok(Expr::Call {
                function: function.clone(),
                args: inlined,
            })
        }
        // Java `default -> throw`: Str, ArrayAccess, Range, ArrayLiteral.
        _ => Err(FreesError::solver(format!(
            "In {}: unsupported construct inside an Integral with variable limits.",
            ie.original.source_text
        ))),
    }
}

// ---------------------------------------------------------------------------
// The stepping driver (constant limits)
// ---------------------------------------------------------------------------

/// Second-order predictor–corrector integration with adaptive step sizing
/// (Heun's method). The integrand receives `(t, F)` where `F` is the running
/// integral value, so `dF/dt = f(t, F)` initial-value problems work too.
///
/// Per step: predictor `F_p = F + h·f(t, F)` (Euler), corrector
/// `F_c = F + h·(f(t, F) + f(t+h, F_p))/2` (trapezoid); their difference
/// estimates the local error and drives halving/doubling of `h`. A positive
/// `fixed_step` disables adaptation.
///
/// Port of `IntegralSolver.integrate`, minus its `deadlineNanos` check (see the
/// module docs — there is no clock on `wasm32-unknown-unknown`).
///
/// # Errors
///
/// [`FreesError::Solver`] when the sweep exceeds [`MAX_STEPS`], plus whatever
/// `integrand` itself raises (in the engine driver, a subsystem that will not
/// solve at some `t`).
pub fn integrate<F>(mut integrand: F, lower: f64, upper: f64, fixed_step: f64) -> Result<f64>
where
    F: FnMut(f64, f64) -> Result<f64>,
{
    if lower == upper {
        return Ok(0.0);
    }
    let direction = (upper - lower).signum();
    let span = (upper - lower).abs();
    let adaptive = fixed_step <= 0.0;
    let mut h = if adaptive {
        (upper - lower) / INITIAL_STEPS as f64
    } else {
        fixed_step.min(span) * direction
    };
    let h_max = span / 4.0;
    let h_min = span * 1.0e-9;

    let mut t = lower;
    let mut total = 0.0;
    let mut f_left = integrand(lower, total)?;
    let mut steps = 0usize;
    while direction * (upper - t) > span * 1.0e-12 {
        steps += 1;
        if steps > MAX_STEPS {
            return Err(FreesError::solver(format!(
                "Integral did not converge within {MAX_STEPS} steps."
            )));
        }
        if direction * (t + h - upper) > 0.0 {
            h = upper - t;
        }
        let predicted = total + h * f_left;
        let f_right = integrand(t + h, predicted)?;
        let corrected = total + h * (f_left + f_right) / 2.0;
        let error = (corrected - predicted).abs();
        let scale = corrected.abs().max(ABS_FLOOR);
        if adaptive && error > REL_TOL * scale && h.abs() > h_min {
            h /= 2.0;
            continue;
        }
        total = corrected;
        t += h;
        f_left = integrand(t, total)?;
        if adaptive && error < REL_TOL * scale / 16.0 {
            h = direction * (h.abs() * 2.0).min(h_max);
        }
    }
    Ok(total)
}

// ---------------------------------------------------------------------------
// In-expression quadrature: Integral (adaptive Simpson)
// ---------------------------------------------------------------------------

/// `Evaluator.SIMPSON_REL_TOL` — the noise floor must stay far below the
/// perturbations of the numerical Jacobian, hence the tight tolerance.
const SIMPSON_REL_TOL: f64 = 1.0e-12;
/// `Evaluator.SIMPSON_MAX_DEPTH`.
const SIMPSON_MAX_DEPTH: u32 = 24;

/// `Integral(f, t, a, b[, step])` evaluated **in expression position** —
/// adaptive Simpson over a closed-form integrand, the Java
/// `Evaluator.integralQuadrature`.
///
/// This is the path [`inlined_equation`] produces for variable limits, so it is
/// re-entered at every Newton residual evaluation. `step` is accepted and
/// **ignored**, exactly as the Java arm ignores its fifth argument: a fixed
/// step belongs to the stepping driver ([`integrate`]), which only runs for
/// constant limits.
///
/// Reversed limits (`b < a`) integrate backwards and return the negated value,
/// as adaptive Simpson naturally does and the Java arm allows.
pub fn integral(
    integrand: &Expr,
    var: &str,
    a: f64,
    b: f64,
    step: Option<f64>,
    scope: &Scope,
) -> Result<f64> {
    integral_with(integrand, var, a, b, step, scope, EvalContext::default())
}

/// [`integral`] with a document context, so a user `FUNCTION` or `TABLE` inside
/// the integrand resolves (the Java `Evaluator.eval(expr, values, defs)`
/// triple). [`integral`] is the frozen no-context contract the evaluator
/// currently dispatches to.
pub fn integral_with(
    integrand: &Expr,
    var: &str,
    a: f64,
    b: f64,
    _step: Option<f64>,
    scope: &Scope,
    ctx: EvalContext<'_>,
) -> Result<f64> {
    if a == b {
        return Ok(0.0);
    }
    // The Java mutates the caller's `values` map and restores the binding in a
    // `finally`; an owned copy is the same thing without the restore hazard.
    let mut values = scope.clone();
    let var = var.to_ascii_lowercase();
    let fa = bind_and_eval(integrand, &var, a, &mut values, ctx)?;
    let fm = bind_and_eval(integrand, &var, (a + b) / 2.0, &mut values, ctx)?;
    let fb = bind_and_eval(integrand, &var, b, &mut values, ctx)?;
    let whole = (b - a) / 6.0 * (fa + 4.0 * fm + fb);
    adaptive_simpson(
        integrand,
        &var,
        &mut values,
        ctx,
        Panel { a, b, fa, fm, fb },
        whole,
        SIMPSON_MAX_DEPTH,
    )
}

/// One Simpson panel: its ends and midpoint plus the integrand values there.
/// (The Java passes these as five loose `double` parameters.)
#[derive(Debug, Clone, Copy)]
struct Panel {
    a: f64,
    b: f64,
    fa: f64,
    fm: f64,
    fb: f64,
}

/// `SimpsonContext.evalAt`: bind the integration variable, evaluate.
fn bind_and_eval(
    integrand: &Expr,
    var: &str,
    t: f64,
    values: &mut Scope,
    ctx: EvalContext<'_>,
) -> Result<f64> {
    values.insert(var.to_string(), t);
    eval_with(integrand, values, ctx)
}

/// `SimpsonContext.adaptiveSimpson`.
fn adaptive_simpson(
    integrand: &Expr,
    var: &str,
    values: &mut Scope,
    ctx: EvalContext<'_>,
    panel: Panel,
    whole: f64,
    depth: u32,
) -> Result<f64> {
    let Panel { a, b, fa, fm, fb } = panel;
    let m = (a + b) / 2.0;
    let lm = (a + m) / 2.0;
    let rm = (m + b) / 2.0;
    let flm = bind_and_eval(integrand, var, lm, values, ctx)?;
    let frm = bind_and_eval(integrand, var, rm, values, ctx)?;
    let left = (m - a) / 6.0 * (fa + 4.0 * flm + fm);
    let right = (b - m) / 6.0 * (fm + 4.0 * frm + fb);
    let halves = left + right;
    let delta = halves - whole;
    if depth == 0 || delta.abs() <= 15.0 * SIMPSON_REL_TOL * halves.abs().max(1.0) {
        return Ok(halves + delta / 15.0);
    }
    let lower_half = adaptive_simpson(
        integrand,
        var,
        values,
        ctx,
        Panel {
            a,
            b: m,
            fa,
            fm: flm,
            fb: fm,
        },
        left,
        depth - 1,
    )?;
    let upper_half = adaptive_simpson(
        integrand,
        var,
        values,
        ctx,
        Panel {
            a: m,
            b,
            fa: fm,
            fm: frm,
            fb,
        },
        right,
        depth - 1,
    )?;
    Ok(lower_half + upper_half)
}

// ---------------------------------------------------------------------------
// In-expression quadrature: GaussIntegral (iterative Gauss–Legendre)
// ---------------------------------------------------------------------------

/// Apache's default `points` per panel (`Evaluator.gaussLegendreQuadrature`).
const GAUSS_DEFAULT_POINTS: usize = 5;
/// `IterativeLegendreGaussIntegrator(points, 1e-10, 1e-10, 2, 64)`.
const GAUSS_REL_ACCURACY: f64 = 1.0e-10;
const GAUSS_ABS_ACCURACY: f64 = 1.0e-10;
const GAUSS_MIN_ITERATIONS: usize = 2;
const GAUSS_MAX_ITERATIONS: usize = 64;
/// Panel-count ceiling. Apache bounds the run with an elapsed-time/evaluation
/// budget the wasm target cannot provide; a divergent integrand would otherwise
/// spin the worker forever, so the panel count is capped instead.
const GAUSS_MAX_PANELS: usize = 1 << 20;

/// `GaussIntegral(f, t, a, b[, points])`: iterative Gauss–Legendre quadrature
/// of `f` over `t ∈ [a, b]`, with `t` bound inside `f` only (mirroring
/// [`Expr::variables`]' special case).
///
/// Port of `Evaluator.gaussLegendreQuadrature`, i.e. Apache Commons Math's
/// `IterativeLegendreGaussIntegrator(points, 1e-10, 1e-10, 2, 64)`: panel the
/// interval into `n` equal pieces, apply the `points`-node Legendre rule
/// (Kahan-summed) to each, and grow `n` until two successive estimates agree.
///
/// # Errors
///
/// [`FreesError::Evaluation`] for reversed limits — Apache's
/// `verifyInterval` rejects `lower >= upper` and the Java engine propagates
/// that as `NumberIsTooLargeException: endpoints do not specify an interval` —
/// and when the iteration budget is exhausted.
pub fn gauss_integral(
    integrand: &Expr,
    var: &str,
    a: f64,
    b: f64,
    points: Option<usize>,
    scope: &Scope,
) -> Result<f64> {
    gauss_integral_with(integrand, var, a, b, points, scope, EvalContext::default())
}

/// [`gauss_integral`] with a document context — see [`integral_with`].
pub fn gauss_integral_with(
    integrand: &Expr,
    var: &str,
    a: f64,
    b: f64,
    points: Option<usize>,
    scope: &Scope,
    ctx: EvalContext<'_>,
) -> Result<f64> {
    if a == b {
        return Ok(0.0);
    }
    // Apache's `UnivariateSolverUtils.verifyInterval`: `lower >= upper` throws.
    // The comparison is deliberately *not* negated — `NaN >= x` is false in
    // Java too, so a NaN limit falls through to the rule and produces NaN,
    // which is the non-finite residual Newton's line search already handles.
    if a >= b {
        return Err(FreesError::evaluation(format!(
            "GaussIntegral: endpoints do not specify an interval: [{a}, {b}]"
        )));
    }
    let n = points.unwrap_or(GAUSS_DEFAULT_POINTS).clamp(2, 64);
    let (nodes, weights) = legendre_rule(n);
    let mut values = scope.clone();
    let var = var.to_ascii_lowercase();

    let stage = |panels: usize, values: &mut Scope| -> Result<f64> {
        let step = (b - a) / panels as f64;
        let mut sum = 0.0;
        for i in 0..panels {
            let lo = a + i as f64 * step;
            let hi = lo + step;
            sum += gauss_panel(&nodes, &weights, lo, hi, integrand, &var, values, ctx)?;
        }
        Ok(sum)
    };

    let mut old = stage(1, &mut values)?;
    let mut panels = 2usize;
    let mut iterations = 0usize;
    loop {
        let current = stage(panels, &mut values)?;
        let delta = (current - old).abs();
        let limit = GAUSS_ABS_ACCURACY.max(GAUSS_REL_ACCURACY * (old.abs() + current.abs()) * 0.5);
        if iterations + 1 >= GAUSS_MIN_ITERATIONS && delta <= limit {
            return Ok(current);
        }
        let ratio = 4.0f64.min((delta / limit).powf(0.5 / n as f64));
        panels = ((ratio * panels as f64) as usize).max(panels + 1);
        old = current;
        iterations += 1;
        if iterations > GAUSS_MAX_ITERATIONS || panels > GAUSS_MAX_PANELS {
            return Err(FreesError::evaluation(
                "GaussIntegral did not converge; the integrand may be singular \
                 or discontinuous on the interval.",
            ));
        }
    }
}

/// One Legendre panel over `[lo, hi]`: the `[-1, 1]` rule affinely transformed
/// (Apache `GaussIntegratorFactory.transform`) and summed with Kahan
/// compensation (`GaussIntegrator.integrate`).
#[allow(clippy::too_many_arguments)]
fn gauss_panel(
    nodes: &[f64],
    weights: &[f64],
    lo: f64,
    hi: f64,
    integrand: &Expr,
    var: &str,
    values: &mut Scope,
    ctx: EvalContext<'_>,
) -> Result<f64> {
    let scale = (hi - lo) / 2.0;
    let shift = lo + scale;
    let mut s = 0.0;
    let mut c = 0.0;
    for (node, weight) in nodes.iter().zip(weights) {
        let x = node * scale + shift;
        let w = weight * scale;
        let y = w * bind_and_eval(integrand, var, x, values, ctx)? - c;
        let t = s + y;
        c = (t - s) - y;
        s = t;
    }
    Ok(s)
}

// ---------------------------------------------------------------------------
// Double-double arithmetic, for the Legendre rule only
// ---------------------------------------------------------------------------

/// An unevaluated sum `hi + lo` of two non-overlapping `f64`s — about 106 bits
/// of significand.
///
/// Apache builds its Gauss–Legendre rule in `BigDecimal`
/// (`LegendreHighPrecisionRuleFactory`) and rounds to `f64` once at the end.
/// Plain `f64` cannot reproduce that: the three-term recurrence and the weight
/// formula `2 / ((1 - z²)·P'ₙ(z)²)` each shed an ulp or two, and the resulting
/// rule integrates `∫₀¹ t² dt` to `0.33333333333333326` where the oracle
/// answers `0.3333333333333333`. Building the rule in this compensated format
/// and rounding once recovers the oracle's doubles exactly.
///
/// The operations are the standard error-free transformations (Dekker/Knuth) —
/// safe Rust, no dependencies. Only what the rule needs is implemented.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Dd {
    hi: f64,
    lo: f64,
}

/// `a + b` exactly, as an unevaluated pair. Knuth's TwoSum.
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let b_virtual = s - a;
    (s, (a - (s - b_virtual)) + (b - b_virtual))
}

/// TwoSum for the case `|a| >= |b|` (Dekker's FastTwoSum).
fn quick_two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    (s, b - (s - a))
}

/// `a * b` exactly, as an unevaluated pair, via a fused multiply-add.
fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    (p, a.mul_add(b, -p))
}

impl Dd {
    fn from(value: f64) -> Dd {
        Dd { hi: value, lo: 0.0 }
    }

    /// Round back to the nearest `f64`.
    fn value(self) -> f64 {
        self.hi + self.lo
    }
}

impl std::ops::Neg for Dd {
    type Output = Dd;
    fn neg(self) -> Dd {
        Dd {
            hi: -self.hi,
            lo: -self.lo,
        }
    }
}

impl std::ops::Add for Dd {
    type Output = Dd;
    fn add(self, other: Dd) -> Dd {
        let (s, e) = two_sum(self.hi, other.hi);
        let e = e + self.lo + other.lo;
        let (hi, lo) = quick_two_sum(s, e);
        Dd { hi, lo }
    }
}

impl std::ops::Sub for Dd {
    type Output = Dd;
    fn sub(self, other: Dd) -> Dd {
        self + (-other)
    }
}

impl std::ops::Mul for Dd {
    type Output = Dd;
    fn mul(self, other: Dd) -> Dd {
        let (p, e) = two_prod(self.hi, other.hi);
        let e = e + (self.hi * other.lo + self.lo * other.hi);
        let (hi, lo) = quick_two_sum(p, e);
        Dd { hi, lo }
    }
}

impl std::ops::Div for Dd {
    type Output = Dd;
    fn div(self, other: Dd) -> Dd {
        // Three Newton-style correction terms, the standard QD long division.
        let q1 = self.hi / other.hi;
        let remainder = self - other * Dd::from(q1);
        let q2 = remainder.hi / other.hi;
        let remainder = remainder - other * Dd::from(q2);
        let q3 = remainder.hi / other.hi;
        let (hi, lo) = quick_two_sum(q1, q2);
        Dd { hi, lo } + Dd::from(q3)
    }
}

// ---------------------------------------------------------------------------
// The Legendre rule
// ---------------------------------------------------------------------------

/// `(P_n(z), P'_n(z))` from the three-term recurrence
/// `j·P_j = (2j−1)·z·P_{j−1} − (j−1)·P_{j−2}` and
/// `P'_n(z) = n·(z·P_n − P_{n−1}) / (z² − 1)`.
fn legendre(n: usize, z: Dd) -> (Dd, Dd) {
    let mut p1 = Dd::from(1.0); // P_j
    let mut p2 = Dd::from(0.0); // P_{j-1}
    for j in 1..=n {
        let p3 = p2;
        p2 = p1;
        let j = j as f64;
        p1 = (Dd::from(2.0 * j - 1.0) * z * p2 - Dd::from(j - 1.0) * p3) / Dd::from(j);
    }
    let dp = Dd::from(n as f64) * (z * p1 - p2) / (z * z - Dd::from(1.0));
    (p1, dp)
}

/// The `n`-point Gauss–Legendre nodes and weights on `[-1, 1]`, in increasing
/// node order — bit-for-bit the doubles Apache's
/// `GaussIntegratorFactory.legendreHighPrecision(n, -1, 1)` returns.
///
/// Roots of `P_n` are found by Newton's method from the Chebyshev estimate
/// `cos(π(i + ¾)/(n + ½))`, evaluated in [`Dd`]. Two structural properties of
/// the high-precision factory are reproduced exactly rather than approached:
/// the rule is symmetric (a node and its mirror share a weight bit-for-bit),
/// and an odd rule's central node is **exactly** zero, not a residual ~1e-17.
fn legendre_rule(n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut nodes = vec![0.0f64; n];
    let mut weights = vec![0.0f64; n];
    let one = Dd::from(1.0);
    let two = Dd::from(2.0);
    let half = n / 2;
    for i in 0..half {
        let estimate = (std::f64::consts::PI * (i as f64 + 0.75) / (n as f64 + 0.5)).cos();
        let mut z = Dd::from(estimate);
        // Newton doubles the correct digits per step; ~6 reach the Dd floor.
        for _ in 0..64 {
            let (p, dp) = legendre(n, z);
            let step = p / dp;
            z = z - step;
            if step.hi.abs() <= 1.0e-31 * z.hi.abs() {
                break;
            }
        }
        let (_, dp) = legendre(n, z);
        let weight = two / ((one - z * z) * dp * dp);
        let node = z.value();
        let weight = weight.value();
        nodes[i] = -node;
        nodes[n - 1 - i] = node;
        weights[i] = weight;
        weights[n - 1 - i] = weight;
    }
    if n % 2 == 1 {
        // P_n(0) = 0 for odd n, and P'_n(0) = n·P_{n-1}(0).
        let (_, dp) = legendre(n, Dd::from(0.0));
        nodes[half] = 0.0;
        weights[half] = (two / (dp * dp)).value();
    }
    (nodes, weights)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinOp;

    fn defs() -> Definitions {
        Definitions::default()
    }

    fn parse_eqs(source: &str) -> Vec<Equation> {
        let doc = crate::parser::parse_document(source).expect("parses");
        doc.equations().into_iter().cloned().collect()
    }

    fn texts(equations: &[Equation]) -> Vec<&str> {
        equations.iter().map(|e| e.source_text.as_str()).collect()
    }

    /// Parse a bare expression by parsing a one-equation document and keeping
    /// its right-hand side (the parser exposes no string-level expression
    /// entry point).
    fn parse_expression(source: &str) -> Expr {
        parse_eqs(&format!("zzz_lhs = {source}\n"))
            .pop()
            .expect("one equation")
            .rhs
    }

    // ----- hoisting ---------------------------------------------------------

    #[test]
    fn a_document_without_integrals_passes_through_unchanged() {
        let equations = parse_eqs("a = 2\nb = a * sin(a)\nc = b + 1\n");
        let hoisted = hoist_nested(equations.clone());
        assert_eq!(hoisted, equations);
    }

    #[test]
    fn hoist_nested_leaves_the_alone_form_untouched() {
        let equations = parse_eqs("F = Integral(t, t, 0, 1)\n");
        assert_eq!(hoist_nested(equations.clone()), equations);
    }

    #[test]
    fn hoist_nested_introduces_a_synthetic_equation() {
        let hoisted = hoist_nested(parse_eqs("y = 1 + Integral(t, t, 0, 1)\n"));
        assert_eq!(hoisted.len(), 2);
        // `y = 1 + integral_1`
        assert_eq!(hoisted[0].lhs, Expr::var("y"));
        assert_eq!(
            hoisted[0].rhs,
            Expr::bin(BinOp::Add, Expr::num(1.0), Expr::var("integral_1"))
        );
        // `integral_1 = Integral(t, t, 0, 1)`, quoting the original line.
        assert_eq!(hoisted[1].lhs, Expr::var("integral_1"));
        assert!(is_integral_call(&hoisted[1].rhs));
        assert_eq!(hoisted[1].source_text, "y = 1 + Integral(t, t, 0, 1)");
    }

    #[test]
    fn fresh_names_avoid_variables_the_document_already_uses() {
        let hoisted = hoist_nested(parse_eqs(
            "integral_1 = 4\ny = integral_1 + Integral(t, t, 0, 1)\n",
        ));
        // `integral_1` is taken, so the synthetic name is `integral_2`.
        assert!(hoisted.iter().any(|eq| eq.lhs == Expr::var("integral_2")));
    }

    // ----- extraction -------------------------------------------------------

    #[test]
    fn extract_reads_constant_limits() {
        let equations = parse_eqs("F = Integral(2*t, t, 0, 1)\n");
        let integrals = extract(&equations, &defs()).expect("extracts");
        assert_eq!(integrals.len(), 1);
        let ie = &integrals[0];
        assert!(ie.constant_limits());
        assert_eq!(ie.lower(), 0.0);
        assert_eq!(ie.upper(), 1.0);
        assert_eq!(ie.result_var, "f");
        assert_eq!(ie.integration_var, "t");
        assert_eq!(ie.fixed_step, 0.0);
    }

    #[test]
    fn extract_leaves_a_variable_limit_unresolved() {
        let equations = parse_eqs("F = Integral(2*t, t, 0, b)\nF = 9\n");
        let integrals = extract(&equations, &defs()).expect("extracts");
        assert_eq!(integrals.len(), 1);
        assert!(!integrals[0].constant_limits());
        assert_eq!(integrals[0].upper_const, None);
        assert_eq!(integrals[0].lower_const, Some(0.0));
    }

    #[test]
    fn constant_limits_may_be_arithmetic_on_builtin_constants() {
        let equations = parse_eqs("F = Integral(sin(t), t, 0, pi#)\n");
        let integrals = extract(&equations, &defs()).expect("extracts");
        assert!(integrals[0].constant_limits());
        assert_eq!(integrals[0].upper(), std::f64::consts::PI);
    }

    #[test]
    fn extract_rejects_a_wrong_argument_count() {
        let err = extract(&parse_eqs("F = Integral(t^2, t, 0)\n"), &defs()).unwrap_err();
        assert!(
            err.to_string_message()
                .starts_with("Integral expects Integral(f, t, lower, upper[, step])"),
            "{err}"
        );
    }

    #[test]
    fn extract_rejects_a_non_variable_integration_variable() {
        let err = extract(&parse_eqs("F = Integral(t^2, 5, 0, 1)\n"), &defs()).unwrap_err();
        assert!(
            err.to_string_message()
                .contains("second argument of Integral must be the integration variable"),
            "{err}"
        );
    }

    #[test]
    fn extract_rejects_a_non_constant_step() {
        let err = extract(&parse_eqs("F = Integral(t^2, t, 0, 1, h)\n"), &defs()).unwrap_err();
        assert!(
            err.to_string_message()
                .contains("step size of Integral must be a numeric constant"),
            "{err}"
        );
    }

    #[test]
    fn extract_rejects_an_integral_that_is_not_alone() {
        // hoist_nested normally prevents this; a hand-built system can still
        // reach it (an Integral inside an array index, say).
        let equations = parse_eqs("y = 1 + Integral(t, t, 0, 1)\n");
        let err = extract(&equations, &defs()).unwrap_err();
        assert!(
            err.to_string_message()
                .starts_with("Integral must appear alone on one side of an equation"),
            "{err}"
        );
    }

    // ----- structural view --------------------------------------------------

    #[test]
    fn structural_view_pins_the_result_and_the_integration_variable() {
        let equations = parse_eqs("F = Integral(t^2, t, 0, 1)\n");
        let integrals = extract(&equations, &defs()).unwrap();
        let view = structural_view(&equations, &integrals).unwrap();
        // The integral equation is gone; a placeholder and a pin replace it.
        assert_eq!(view.len(), 2);
        assert_eq!(
            view[0],
            Equation::new(Expr::var("f"), Expr::num(0.0), "F = Integral(t^2, t, 0, 1)")
        );
        assert_eq!(view[1].lhs, Expr::var("t"));
        assert_eq!(view[1].rhs, Expr::num(0.0));
        assert_eq!(view[1].source_text, "t (integration variable)");
    }

    #[test]
    fn structural_view_pins_each_integration_variable_once() {
        let equations = parse_eqs("F = Integral(t, t, 0, 1)\nG = Integral(t^2, t, 0, 1)\n");
        let integrals = extract(&equations, &defs()).unwrap();
        let view = structural_view(&equations, &integrals).unwrap();
        // two placeholders + exactly one pin
        assert_eq!(view.len(), 3);
        let pins = view
            .iter()
            .filter(|eq| eq.source_text == "t (integration variable)")
            .count();
        assert_eq!(pins, 1);
    }

    #[test]
    fn structural_view_keeps_ordinary_equations_first_and_intact() {
        let equations = parse_eqs("x = 2*t\nF = Integral(x^2, t, 0, 1)\n");
        let integrals = extract(&equations, &defs()).unwrap();
        let view = structural_view(&equations, &integrals).unwrap();
        assert_eq!(view[0], equations[0]);
        assert_eq!(
            texts(&view),
            vec![
                "x = 2*t",
                "F = Integral(x^2, t, 0, 1)",
                "t (integration variable)"
            ]
        );
    }

    #[test]
    fn structural_view_of_a_variable_limit_uses_the_inlined_equation() {
        let equations = parse_eqs("F = Integral(2*t, t, 0, b)\nF = 9\n");
        let integrals = extract(&equations, &defs()).unwrap();
        let view = structural_view(&equations, &integrals).unwrap();
        // ordinary (`F = 9`) + inlined + pin `t = b`
        assert_eq!(view.len(), 3);
        assert!(is_integral_call(&view[1].rhs));
        assert_eq!(view[2].lhs, Expr::var("t"));
        assert_eq!(view[2].rhs, Expr::var("b"));
    }

    // ----- inlining ---------------------------------------------------------

    #[test]
    fn inlining_substitutes_a_t_dependent_definition() {
        let equations = parse_eqs("g = 3 * t^2\nF = Integral(g, t, 0, b)\nF = 8\n");
        let integrals = extract(&equations, &defs()).unwrap();
        let ordinary = ordinary_equations(&equations, &integrals);
        let inlined = inlined_equation(&integrals[0], &ordinary).unwrap();
        let Expr::Call { function, args } = &inlined.rhs else {
            panic!("expected an integral call, got {:?}", inlined.rhs);
        };
        assert_eq!(function, FUNCTION_NAME);
        // `g` was replaced by `3 * t^2`.
        assert_eq!(args[0], equations[0].rhs);
        assert_eq!(args[1], Expr::var("t"));
        assert_eq!(args[3], Expr::var("b"));
    }

    #[test]
    fn inlining_leaves_t_independent_variables_alone() {
        let equations = parse_eqs("k = 4\nF = Integral(k * t, t, 0, b)\nF = 8\n");
        let integrals = extract(&equations, &defs()).unwrap();
        let ordinary = ordinary_equations(&equations, &integrals);
        let inlined = inlined_equation(&integrals[0], &ordinary).unwrap();
        let Expr::Call { args, .. } = &inlined.rhs else {
            panic!("expected an integral call");
        };
        // `k` stays a system unknown rather than being folded to 4.
        assert_eq!(
            args[0],
            Expr::bin(BinOp::Mul, Expr::var("k"), Expr::var("t"))
        );
    }

    #[test]
    fn inlining_rejects_an_integrand_referencing_its_own_result() {
        let equations = parse_eqs("F = Integral(F, t, 0, b)\nb = 2\n");
        let integrals = extract(&equations, &defs()).unwrap();
        let ordinary = ordinary_equations(&equations, &integrals);
        let err = inlined_equation(&integrals[0], &ordinary).unwrap_err();
        assert!(
            err.to_string_message()
                .starts_with("An Integral with variable limits cannot reference its own result"),
            "{err}"
        );
    }

    #[test]
    fn inlining_rejects_a_circular_definition_chain() {
        let equations = parse_eqs("F = Integral(x, t, 0, b)\nx = y + t\ny = x\nF = 9\n");
        let integrals = extract(&equations, &defs()).unwrap();
        let ordinary = ordinary_equations(&equations, &integrals);
        let err = inlined_equation(&integrals[0], &ordinary).unwrap_err();
        let message = err.to_string_message();
        assert!(
            message.contains("'x' depends on the integration variable t"),
            "{message}"
        );
        assert!(message.contains("no explicit definition"), "{message}");
    }

    #[test]
    fn an_ambiguously_defined_variable_is_left_alone_not_inlined() {
        // `w` is stated twice, so `explicitDefinitions` drops it — and with no
        // definition, `dependsOnIntegrationVar` cannot see through to `t`, so
        // the inliner leaves `w` standing. (The Java engine then rejects the
        // *whole document* as overspecified, which is the blocker's job, not
        // this pass's: verified against the oracle.)
        let equations = parse_eqs("F = Integral(w, t, 0, b)\nw = t\nw = 2*t\nF = 9\n");
        let integrals = extract(&equations, &defs()).unwrap();
        let ordinary = ordinary_equations(&equations, &integrals);
        let inlined = inlined_equation(&integrals[0], &ordinary).unwrap();
        let Expr::Call { args, .. } = &inlined.rhs else {
            panic!("expected an integral call");
        };
        assert_eq!(args[0], Expr::var("w"));
    }

    #[test]
    fn inlining_rejects_a_construct_it_cannot_substitute_into() {
        // A string literal has no `inline` arm in the Java switch, so it falls
        // to the `default -> throw`.
        let equations = parse_eqs("F = Integral('x', t, 0, b)\nF = 8\n");
        let integrals = extract(&equations, &defs()).unwrap();
        let ordinary = ordinary_equations(&equations, &integrals);
        let err = inlined_equation(&integrals[0], &ordinary).unwrap_err();
        assert!(
            err.to_string_message()
                .ends_with("unsupported construct inside an Integral with variable limits."),
            "{err}"
        );
    }

    // ----- the stepping driver ---------------------------------------------

    #[test]
    fn integrate_over_a_zero_span_is_zero() {
        let value = integrate(|t, _| Ok(t), 1.0, 1.0, 0.0).unwrap();
        assert_eq!(value, 0.0);
    }

    #[test]
    fn integrate_matches_the_java_stepper_on_a_polynomial() {
        // The oracle value for `F = Integral(t^2, t, 0, 1)`, carrying the
        // stepper's truncation error rather than being exactly 1/3.
        let value = integrate(|t, _| Ok(t * t), 0.0, 1.0, 0.0).unwrap();
        assert_eq!(value, 0.333_333_336_004_113_86);
    }

    #[test]
    fn integrate_honours_a_fixed_step() {
        let value = integrate(|t, _| Ok(t), 0.0, 2.0, 0.01).unwrap();
        assert!((value - 2.0).abs() < 1e-9, "{value}");
    }

    #[test]
    fn integrate_runs_backwards_for_reversed_limits() {
        let forward = integrate(|t, _| Ok(t * t), 0.0, 1.0, 0.0).unwrap();
        let backward = integrate(|t, _| Ok(t * t), 1.0, 0.0, 0.0).unwrap();
        assert!(backward < 0.0, "{backward}");
        assert!((backward + forward).abs() < 1e-6, "{backward} vs {forward}");
    }

    #[test]
    fn integrate_solves_an_initial_value_problem_through_the_running_total() {
        // dF/dt = -0.5*(F + 1), F(0) = 0  =>  F(2) = e^-1 - 1
        let value = integrate(|_, f| Ok(-0.5 * (f + 1.0)), 0.0, 2.0, 0.0).unwrap();
        assert!(
            (value - (std::f64::consts::E.recip() - 1.0)).abs() < 1e-3,
            "{value}"
        );
    }

    #[test]
    fn integrate_propagates_an_integrand_failure() {
        let err = integrate(
            |t, _| {
                if t > 0.5 {
                    Err(FreesError::solver("subsystem did not converge"))
                } else {
                    Ok(1.0)
                }
            },
            0.0,
            1.0,
            0.0,
        )
        .unwrap_err();
        assert!(err.to_string_message().contains("subsystem"), "{err}");
    }

    // ----- in-expression quadrature ----------------------------------------

    fn quad(source: &str, var: &str, a: f64, b: f64) -> f64 {
        let expr = parse_expression(source);
        integral(&expr, var, a, b, None, &Scope::new()).expect("integrates")
    }

    fn gauss(source: &str, var: &str, a: f64, b: f64, points: Option<usize>) -> f64 {
        let expr = parse_expression(source);
        gauss_integral(&expr, var, a, b, points, &Scope::new()).expect("integrates")
    }

    #[test]
    fn adaptive_simpson_is_exact_on_a_polynomial() {
        assert!((quad("t^2", "t", 0.0, 1.0) - 1.0 / 3.0).abs() < 1e-12);
        assert!((quad("2*t", "t", 0.0, 3.0) - 9.0).abs() < 1e-12);
    }

    #[test]
    fn adaptive_simpson_integrates_sin_over_a_half_period() {
        let value = quad("sin(t)", "t", 0.0, std::f64::consts::PI);
        assert!((value - 2.0).abs() < 1e-11, "{value}");
    }

    #[test]
    fn adaptive_simpson_negates_for_reversed_limits() {
        let value = quad("t^2", "t", 1.0, 0.0);
        assert!((value + 1.0 / 3.0).abs() < 1e-12, "{value}");
    }

    #[test]
    fn equal_limits_integrate_to_zero_in_both_kernels() {
        assert_eq!(quad("t^2", "t", 2.0, 2.0), 0.0);
        assert_eq!(gauss("t^2", "t", 2.0, 2.0, None), 0.0);
    }

    #[test]
    fn the_integration_variable_does_not_leak_into_the_callers_scope() {
        let mut scope = Scope::new();
        scope.insert("t".into(), 42.0);
        let expr = parse_expression("t^2");
        assert!((integral(&expr, "t", 0.0, 1.0, None, &scope).unwrap() - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(scope["t"], 42.0);
    }

    #[test]
    fn the_integrand_sees_the_callers_other_variables() {
        let mut scope = Scope::new();
        scope.insert("k".into(), 3.0);
        let expr = parse_expression("k * t");
        let value = integral(&expr, "t", 0.0, 2.0, None, &scope).unwrap();
        assert!((value - 6.0).abs() < 1e-12, "{value}");
    }

    #[test]
    fn gauss_legendre_matches_the_java_oracle() {
        // Oracle values from the Java engine (see docs/status-phase4.md).
        assert_eq!(gauss("t^2", "t", 0.0, 1.0, None), 0.333_333_333_333_333_3);
        assert_eq!(
            gauss("sin(x)", "x", 0.0, std::f64::consts::PI, None),
            2.000_000_000_001_303
        );
        assert_eq!(
            gauss("exp(x)", "x", 0.0, 1.0, Some(7)),
            std::f64::consts::E - 1.0
        );
    }

    #[test]
    fn gauss_legendre_rejects_reversed_limits_like_apache() {
        let expr = parse_expression("t^2");
        let err = gauss_integral(&expr, "t", 1.0, 0.0, None, &Scope::new()).unwrap_err();
        assert!(
            err.to_string_message()
                .contains("endpoints do not specify an interval"),
            "{err}"
        );
    }

    #[test]
    fn the_legendre_rule_is_symmetric_and_sums_to_two() {
        for n in 2..=64 {
            let (nodes, weights) = legendre_rule(n);
            assert_eq!(nodes.len(), n);
            let sum: f64 = weights.iter().sum();
            assert!((sum - 2.0).abs() < 1e-13, "n = {n}: weights sum to {sum}");
            for i in 0..n {
                // mirrored nodes, identical weights
                assert_eq!(nodes[i], -nodes[n - 1 - i], "n = {n}, i = {i}");
                assert_eq!(weights[i], weights[n - 1 - i], "n = {n}, i = {i}");
                if i + 1 < n {
                    assert!(nodes[i] < nodes[i + 1], "n = {n}: nodes not increasing");
                }
            }
            if n % 2 == 1 {
                assert_eq!(nodes[n / 2], 0.0, "n = {n}: central node must be exact");
            }
        }
    }

    #[test]
    fn a_gauss_rule_of_n_points_is_exact_to_degree_2n_minus_1() {
        // ∫₀¹ x^9 dx = 1/10 — degree 9, exactly the 5-point rule's limit.
        let value = gauss("x^9", "x", 0.0, 1.0, Some(5));
        assert!((value - 0.1).abs() < 1e-14, "{value}");
    }
}
