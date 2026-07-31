//! `LINEARIZE … END` — plant → control coupling.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/ast/LinearizeSystem.java`
//! together with the numeric half that lives in
//! `core/ode/DynamicSolver.linearize` and the injection half in
//! `core/EquationSystemSolver.injectLinearizations` / `emitMatrix` /
//! `extractScalarConstants` / `invertDisplayNames`.
//!
//! # What it produces
//!
//! A `LINEARIZE` block names a transient component network (a `DYNAMIC` block)
//! and the exogenous `INPUT`s / observed `OUTPUT`s. At solve time the network is
//! numerically linearized about its **initial-condition operating point** into
//! the state-space matrices
//!
//! ```text
//! A = ∂ẋ/∂x    B = ∂ẋ/∂u    C = ∂y/∂x    D = ∂y/∂u
//! ```
//!
//! (states in `der()` order), which are then injected as `A[i,j] = value`
//! equations so a following `CALL lqr/place/ss(...)` in the *same document*
//! reads them. For a linear plant the finite differences are exact at any point.
//!
//! # Why it lives under `dae/`
//!
//! The computation is a finite-difference sweep over the same per-step algebraic
//! solve `ẋ = f(x, u)` that the DAE residual is built from — it is the DAE's
//! Jacobian, evaluated in the model's own variables instead of the integrator's.
//! It needs no integration at all.
//!
//! # Verified against the oracle
//!
//! `fixtures/corpus/linearize-*.frees` are solved by the real Java engine and
//! the emitted `A[i,j]` values are the goldens the tests below replay. The
//! second one is a two-state / two-input / two-output chain, so every matrix
//! shape is exercised rather than just the SISO diagonal.

use crate::ast::{Equation, Expr};
use crate::diag::{FreesError, Result};
use crate::eval::Scope;
use std::collections::BTreeMap;

/// The forward-difference step factor: `ε = 1e-6 · max(|v|, 1)`.
///
/// Transcribed from `DynamicSolver.linearize`; the constant stays as written.
/// It is deliberately looser than the integrator's `1e-7` — the perturbed point
/// is resolved by a *nested* Newton solve, so its answer carries that solve's
/// residual tolerance as noise and a smaller `ε` would amplify it.
const FD_EPS_FACTOR: f64 = 1e-6;

/// A `LINEARIZE … END` block.
///
/// Port of the `LinearizeSystem` record. `inputs`/`outputs` keep the **dotted
/// display spelling** the user wrote (`m.port.T`), lowercased; they are resolved
/// to flat solver names against the expanded component network by
/// [`resolve_names`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinearizeSystem {
    /// Block name, for diagnostics.
    pub name: String,
    /// The `DYNAMIC` block whose component network is linearized.
    pub dynamic_name: String,
    /// Matrix variable name for `A` (the state matrix), and so on.
    pub a_name: String,
    pub b_name: String,
    pub c_name: String,
    pub d_name: String,
    /// Exogenous input variable names (dotted display names).
    pub inputs: Vec<String>,
    /// Observed output variable names (dotted display names).
    pub outputs: Vec<String>,
    pub source_text: String,
}

impl LinearizeSystem {
    /// The header defaults: `a = A, b = B, c = C, d = D`.
    pub const DEFAULT_MATRIX_NAMES: [&'static str; 4] = ["A", "B", "C", "D"];
}

/// The numerically-linearized state-space model of a block at its operating
/// point. Port of `DynamicSolver.Linearization`.
#[derive(Debug, Clone, PartialEq)]
pub struct Linearization {
    pub states: Vec<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    /// `n × n`
    pub a: Vec<Vec<f64>>,
    /// `n × m`
    pub b: Vec<Vec<f64>>,
    /// `p × n`
    pub c: Vec<Vec<f64>>,
    /// `p × m`
    pub d: Vec<Vec<f64>>,
}

/// The operating point a block is linearized about.
///
/// Everything here is what `DynamicSolver` already holds after classification:
/// the block's declared time variable, its `t0`, its state names in `der()`
/// order, their initial values, and the analytic environment the network's
/// parameters were solved into.
pub struct OperatingPoint<'a> {
    pub block_name: &'a str,
    pub time_var: &'a str,
    pub t0: f64,
    /// State names in `der()` order.
    pub states: &'a [String],
    /// State values at the operating point, aligned with [`Self::states`].
    pub x0: &'a [f64],
    /// Values pinned from the analytic solve (parameters, constants). The
    /// exogenous inputs are read from here too.
    pub analytic_values: &'a Scope,
}

impl OperatingPoint<'_> {
    /// The pinned environment one linearization probe is solved in.
    ///
    /// Port of `DynamicSolver.solveForLinearization`'s map construction: the
    /// analytic values, then any input override, then time (the declared time
    /// variable *and* the reserved global `time` alias), then the states.
    pub fn pinned(&self, t: f64, x: &[f64], override_input: Option<(&str, f64)>) -> Scope {
        let mut pinned = self.analytic_values.clone();
        if let Some((name, value)) = override_input {
            pinned.insert(name.to_string(), value);
        }
        pinned.insert(self.time_var.to_string(), t);
        pinned.entry("time".to_string()).or_insert(t);
        for (k, state) in self.states.iter().enumerate() {
            pinned.insert(state.clone(), x[k]);
        }
        pinned
    }
}

/// Linearizes the block about its operating point by finite differences,
/// reusing the per-step algebraic solve `ẋ = f(x, u)`: perturbing each state
/// gives the `A` and `C` columns, each input the `B` and `D` columns.
///
/// Port of `DynamicSolver.linearize`. `algebraic` is the caller's inner solve —
/// `EquationSystemSolver.solvePinned` over the block's algebraic template — which
/// takes a pinned environment and returns the full value map, including each
/// reified `der$X`. Keeping it a parameter is what lets this run without an
/// integrator and without depending on the `DYNAMIC` block owner.
///
/// Inputs are exogenous values pinned in the analytic environment (e.g. a source
/// value); an input the environment does not name is taken as `0.0`, exactly as
/// the Java's `getOrDefault`. Outputs are any solved variables of the network
/// (flat names).
pub fn linearize<F>(
    op: &OperatingPoint<'_>,
    inputs: &[String],
    outputs: &[String],
    mut algebraic: F,
) -> Result<Linearization>
where
    F: FnMut(&Scope) -> Result<Scope>,
{
    let n = op.states.len();
    let m = inputs.len();
    let p = outputs.len();
    if op.x0.len() != n {
        return Err(FreesError::solver(format!(
            "LINEARIZE {}: the operating point has {} state values for {n} states.",
            op.block_name,
            op.x0.len()
        )));
    }
    let x0 = op.x0.to_vec();

    let base = algebraic(&op.pinned(op.t0, &x0, None))?;
    let f0 = der_values_of(&base, op.states);
    let y0v = output_values_of(&base, outputs, op.block_name)?;

    let mut a = vec![vec![0.0; n]; n];
    let mut c = vec![vec![0.0; n]; p];
    for j in 0..n {
        let eps = FD_EPS_FACTOR * x0[j].abs().max(1.0);
        let mut xp = x0.clone();
        xp[j] += eps;
        let v = algebraic(&op.pinned(op.t0, &xp, None))?;
        let fp = der_values_of(&v, op.states);
        let yp = output_values_of(&v, outputs, op.block_name)?;
        for i in 0..n {
            a[i][j] = (fp[i] - f0[i]) / eps;
        }
        for k in 0..p {
            c[k][j] = (yp[k] - y0v[k]) / eps;
        }
    }

    let mut b = vec![vec![0.0; m]; n];
    let mut d = vec![vec![0.0; m]; p];
    for (q, u) in inputs.iter().enumerate() {
        let u0 = op.analytic_values.get(u).copied().unwrap_or(0.0);
        let eps = FD_EPS_FACTOR * u0.abs().max(1.0);
        let v = algebraic(&op.pinned(op.t0, &x0, Some((u, u0 + eps))))?;
        let fp = der_values_of(&v, op.states);
        let yp = output_values_of(&v, outputs, op.block_name)?;
        for i in 0..n {
            b[i][q] = (fp[i] - f0[i]) / eps;
        }
        for k in 0..p {
            d[k][q] = (yp[k] - y0v[k]) / eps;
        }
    }

    Ok(Linearization {
        states: op.states.to_vec(),
        inputs: inputs.to_vec(),
        outputs: outputs.to_vec(),
        a,
        b,
        c,
        d,
    })
}

/// The reified `der$X` value of each state, defaulting to `0.0`.
/// Port of `DynamicSolver.derValuesOf`.
fn der_values_of(values: &Scope, states: &[String]) -> Vec<f64> {
    states
        .iter()
        .map(|s| {
            values
                .get(&crate::dae::assembly::der_var(s))
                .copied()
                .unwrap_or(0.0)
        })
        .collect()
}

/// The observed outputs. Port of `DynamicSolver.outputValuesOf`, including its
/// diagnostic: an output that is not a variable of the network is the user's
/// mistake and is named as such, not silently zeroed.
fn output_values_of(values: &Scope, outputs: &[String], block_name: &str) -> Result<Vec<f64>> {
    outputs
        .iter()
        .map(|o| {
            values.get(o).copied().ok_or_else(|| {
                FreesError::solver(format!(
                    "LINEARIZE: output '{o}' is not a variable of the network '{block_name}'."
                ))
            })
        })
        .collect()
}

/// Emits `name[i,j] = value` equations for a matrix (1-indexed); a
/// single-column matrix also gets the 1-D `name[i]` form so SISO control calls
/// (e.g. `B[1:n]`) resolve.
///
/// Port of `EquationSystemSolver.emitMatrix`. The equation is keyed by the
/// **lowercase** flat name and the display name is registered alongside, which
/// is why the solved result reads back as `A[1,1]` rather than `a[1,1]`.
pub fn emit_matrix(
    out: &mut Vec<Equation>,
    display_names: &mut BTreeMap<String, String>,
    name: &str,
    m: &[Vec<f64>],
) {
    let lower = name.to_lowercase();
    let rows = m.len();
    let cols = if rows > 0 { m[0].len() } else { 0 };
    for (i, row) in m.iter().enumerate() {
        for (j, &value) in row.iter().enumerate().take(cols) {
            let k2 = format!("{lower}[{},{}]", i + 1, j + 1);
            out.push(Equation::new(
                Expr::Var(k2.clone()),
                Expr::num(value),
                format!("{k2} (linearized)"),
            ));
            display_names
                .entry(k2)
                .or_insert_with(|| format!("{name}[{},{}]", i + 1, j + 1));
            if cols == 1 {
                let k1 = format!("{lower}[{}]", i + 1);
                out.push(Equation::new(
                    Expr::Var(k1.clone()),
                    Expr::num(row[0]),
                    format!("{k1} (linearized)"),
                ));
                display_names
                    .entry(k1)
                    .or_insert_with(|| format!("{name}[{}]", i + 1));
            }
        }
    }
}

/// Scalar constant assignments (`var = number`) from the equation list, used as
/// the linearization's exogenous (input) operating-point values.
///
/// Port of `EquationSystemSolver.extractScalarConstants`. An imaginary literal
/// is skipped: a complex constant is not an operating point.
pub fn extract_scalar_constants(equations: &[Equation]) -> Scope {
    let mut c = Scope::new();
    for e in equations {
        if let (
            Expr::Var(n),
            Expr::Num {
                value,
                is_imaginary: false,
                ..
            },
        ) = (&e.lhs, &e.rhs)
        {
            c.insert(n.to_lowercase(), *value);
        }
    }
    c
}

/// `display name → flat name`, both lowercased.
///
/// Port of `EquationSystemSolver.invertDisplayNames`. This is what turns the
/// `m.port.T` a user writes in an `INPUT`/`OUTPUT` list into the network's flat
/// `m$port$t`.
pub fn invert_display_names(display_names: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    display_names
        .iter()
        .map(|(k, v)| (v.to_lowercase(), k.to_lowercase()))
        .collect()
}

/// Resolves a `LINEARIZE` header's dotted display names to flat solver names,
/// leaving a name the network does not know untouched — the Java's
/// `getOrDefault(s, s)`, which lets [`output_values_of`] produce the diagnostic
/// instead of a lookup failure here.
pub fn resolve_names(names: &[String], display_to_flat: &BTreeMap<String, String>) -> Vec<String> {
    names
        .iter()
        .map(|s| display_to_flat.get(s).cloned().unwrap_or_else(|| s.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinOp;

    fn scope(pairs: &[(&str, f64)]) -> Scope {
        pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
    }

    // ── the oracle ──────────────────────────────────────────────────────────
    //
    // From the real Java engine (tools/golden-dumper over the two documents in
    // fixtures/corpus/linearize-*.frees). The physical plant:
    //
    //   HeatSource Q_in ─ ThermalMass M1(C=5000) ─ Conduction w12(k·A/L = 20)
    //                   ─ ThermalMass M2(C=8000) ─ Conduction w2a(k·A/L = 7.5)
    //                   ─ ThermalSource amb(T = Tamb)
    //
    //   C1 Ṫ1 = Q_in − 20 (T1 − T2)
    //   C2 Ṫ2 = 20 (T1 − T2) − 7.5 (T2 − Tamb)
    //
    // so A = [[−20/5000, 20/5000], [20/8000, −27.5/8000]],
    //    B = [[1/5000, 0], [0, 7.5/8000]], C = I, D = 0.
    const ORACLE_A: [[f64; 2]; 2] = [
        [-0.003999999999744949, 0.004000000000222464],
        [0.0024999999998637228, -0.003437500000126827],
    ];
    const ORACLE_B: [[f64; 2]; 2] = [[0.00020000000000575113, 0.0], [0.0, 0.0009374999999691344]];
    const ORACLE_C: [[f64; 2]; 2] = [[0.9999999999384576, 0.0], [0.0, 1.0000000000423648]];
    const ORACLE_SISO_A11: f64 = -0.003999999999744949;
    const ORACLE_SISO_B1: f64 = 0.00019999999997799556;
    const ORACLE_SISO_C11: f64 = 0.9999999999384576;

    /// The inner algebraic solve for the two-mass chain, done in closed form.
    /// This stands in for `EquationSystemSolver.solvePinned`: it takes a pinned
    /// environment and returns the full value map including the `der$` unknowns.
    fn two_mass_algebraic(pinned: &Scope) -> Result<Scope> {
        let t1 = pinned["m1$port$t"];
        let t2 = pinned["m2$port$t"];
        let q = pinned.get("q_in").copied().unwrap_or(0.0);
        let tamb = pinned.get("tamb").copied().unwrap_or(0.0);
        let mut v = pinned.clone();
        v.insert("der$m1$port$t".into(), (q - 20.0 * (t1 - t2)) / 5000.0);
        v.insert(
            "der$m2$port$t".into(),
            (20.0 * (t1 - t2) - 7.5 * (t2 - tamb)) / 8000.0,
        );
        Ok(v)
    }

    fn two_mass_point() -> (Vec<String>, Vec<f64>, Scope) {
        (
            vec!["m1$port$t".into(), "m2$port$t".into()],
            vec![300.0, 310.0],
            scope(&[("q_in", 1000.0), ("tamb", 300.0)]),
        )
    }

    #[test]
    fn two_state_two_input_two_output_matches_the_java_oracle() {
        let (states, x0, analytic) = two_mass_point();
        let op = OperatingPoint {
            block_name: "warmup",
            time_var: "time",
            t0: 0.0,
            states: &states,
            x0: &x0,
            analytic_values: &analytic,
        };
        let inputs = vec!["q_in".to_string(), "tamb".to_string()];
        let outputs = states.clone();
        let lin = linearize(&op, &inputs, &outputs, two_mass_algebraic).unwrap();

        assert_eq!(lin.states, states);
        assert_eq!(lin.a.len(), 2);
        assert_eq!(lin.b[0].len(), 2);
        assert_eq!(lin.c.len(), 2);
        assert_eq!(lin.d.len(), 2);
        for i in 0..2 {
            for j in 0..2 {
                assert!(
                    (lin.a[i][j] - ORACLE_A[i][j]).abs() <= 1e-12,
                    "A[{i}][{j}]: {} vs oracle {}",
                    lin.a[i][j],
                    ORACLE_A[i][j]
                );
                assert!(
                    (lin.b[i][j] - ORACLE_B[i][j]).abs() <= 1e-15,
                    "B[{i}][{j}]: {} vs oracle {}",
                    lin.b[i][j],
                    ORACLE_B[i][j]
                );
                assert!(
                    (lin.c[i][j] - ORACLE_C[i][j]).abs() <= 1e-9,
                    "C[{i}][{j}]: {} vs oracle {}",
                    lin.c[i][j],
                    ORACLE_C[i][j]
                );
                assert_eq!(lin.d[i][j], 0.0, "D[{i}][{j}] must be exactly zero");
            }
        }
    }

    #[test]
    fn the_siso_plant_matches_the_java_oracle() {
        // The single ThermalMass plant of the Java's own ComponentControlTest.
        let states = vec!["m$port$t".to_string()];
        let x0 = vec![300.0];
        let analytic = scope(&[("q_in", 1000.0)]);
        let op = OperatingPoint {
            block_name: "warmup",
            time_var: "time",
            t0: 0.0,
            states: &states,
            x0: &x0,
            analytic_values: &analytic,
        };
        let algebraic = |pinned: &Scope| -> Result<Scope> {
            let t = pinned["m$port$t"];
            let q = pinned.get("q_in").copied().unwrap_or(0.0);
            let mut v = pinned.clone();
            v.insert("der$m$port$t".into(), (q - 20.0 * (t - 300.0)) / 5000.0);
            Ok(v)
        };
        let lin = linearize(&op, &["q_in".to_string()], &states, algebraic).unwrap();
        assert!(
            (lin.a[0][0] - ORACLE_SISO_A11).abs() <= 1e-12,
            "{:?}",
            lin.a
        );
        assert!((lin.b[0][0] - ORACLE_SISO_B1).abs() <= 1e-15, "{:?}", lin.b);
        assert!((lin.c[0][0] - ORACLE_SISO_C11).abs() <= 1e-9, "{:?}", lin.c);
        assert_eq!(lin.d[0][0], 0.0);
    }

    #[test]
    fn an_input_the_environment_does_not_name_is_perturbed_from_zero() {
        // `getOrDefault(u, 0.0)`: eps = 1e-6·max(0,1) = 1e-6, so B still comes
        // out right for a plant whose input has no analytic value.
        let states = vec!["x".to_string()];
        let x0 = vec![0.0];
        let analytic = Scope::new();
        let op = OperatingPoint {
            block_name: "b",
            time_var: "time",
            t0: 0.0,
            states: &states,
            x0: &x0,
            analytic_values: &analytic,
        };
        let algebraic = |pinned: &Scope| -> Result<Scope> {
            let u = pinned.get("u").copied().unwrap_or(0.0);
            let x = pinned["x"];
            let mut v = pinned.clone();
            v.insert("der$x".into(), 3.0 * u - 2.0 * x);
            Ok(v)
        };
        let lin = linearize(&op, &["u".to_string()], &states, algebraic).unwrap();
        assert!((lin.a[0][0] + 2.0).abs() < 1e-9);
        assert!((lin.b[0][0] - 3.0).abs() < 1e-9);
    }

    #[test]
    fn a_state_whose_derivative_is_absent_reads_as_zero() {
        // `derValuesOf` defaults to 0.0 — a state with no der$ in the solve
        // contributes a zero row rather than failing.
        let states = vec!["x".to_string(), "z".to_string()];
        let x0 = vec![1.0, 2.0];
        let analytic = Scope::new();
        let op = OperatingPoint {
            block_name: "b",
            time_var: "time",
            t0: 0.0,
            states: &states,
            x0: &x0,
            analytic_values: &analytic,
        };
        let algebraic = |pinned: &Scope| -> Result<Scope> {
            let mut v = pinned.clone();
            v.insert("der$x".into(), -0.5 * pinned["x"]);
            Ok(v)
        };
        let lin = linearize(&op, &[], &states, algebraic).unwrap();
        assert!((lin.a[0][0] + 0.5).abs() < 1e-9);
        assert_eq!(lin.a[1], vec![0.0, 0.0], "the absent der$ row is zero");
        assert!(
            lin.b.iter().all(|r| r.is_empty()),
            "no inputs, no B columns"
        );
    }

    #[test]
    fn an_output_that_is_not_a_network_variable_is_named_in_the_error() {
        let states = vec!["x".to_string()];
        let x0 = vec![1.0];
        let analytic = Scope::new();
        let op = OperatingPoint {
            block_name: "warmup",
            time_var: "time",
            t0: 0.0,
            states: &states,
            x0: &x0,
            analytic_values: &analytic,
        };
        let algebraic = |pinned: &Scope| -> Result<Scope> {
            let mut v = pinned.clone();
            v.insert("der$x".into(), -pinned["x"]);
            Ok(v)
        };
        let err = linearize(&op, &[], &["nope".to_string()], algebraic)
            .unwrap_err()
            .to_string();
        assert!(err.contains("output 'nope'"), "{err}");
        assert!(err.contains("warmup"), "{err}");
    }

    #[test]
    fn a_failing_inner_solve_propagates() {
        let states = vec!["x".to_string()];
        let x0 = vec![1.0];
        let analytic = Scope::new();
        let op = OperatingPoint {
            block_name: "b",
            time_var: "time",
            t0: 0.0,
            states: &states,
            x0: &x0,
            analytic_values: &analytic,
        };
        let algebraic = |_: &Scope| -> Result<Scope> { Err(FreesError::solver("no convergence")) };
        assert!(linearize(&op, &[], &[], algebraic).is_err());
    }

    #[test]
    fn a_mismatched_operating_point_is_refused() {
        let states = vec!["x".to_string(), "z".to_string()];
        let x0 = vec![1.0];
        let analytic = Scope::new();
        let op = OperatingPoint {
            block_name: "warmup",
            time_var: "time",
            t0: 0.0,
            states: &states,
            x0: &x0,
            analytic_values: &analytic,
        };
        let err = linearize(&op, &[], &[], |_| Ok(Scope::new()))
            .unwrap_err()
            .to_string();
        assert!(err.contains("1 state values for 2 states"), "{err}");
    }

    #[test]
    fn the_probe_pins_time_under_both_names_and_the_states() {
        let states = vec!["temp".to_string()];
        let x0 = vec![7.0];
        let analytic = scope(&[("k", 2.0)]);
        let op = OperatingPoint {
            block_name: "b",
            time_var: "tau",
            t0: 3.0,
            states: &states,
            x0: &x0,
            analytic_values: &analytic,
        };
        let pinned = op.pinned(3.0, &x0, Some(("k", 9.0)));
        assert_eq!(pinned["tau"], 3.0);
        assert_eq!(pinned["time"], 3.0);
        assert_eq!(pinned["temp"], 7.0);
        assert_eq!(
            pinned["k"], 9.0,
            "the override wins over the analytic value"
        );
    }

    #[test]
    fn a_document_variable_named_time_is_not_overwritten_by_the_alias() {
        let analytic = scope(&[("time", 99.0)]);
        let op = OperatingPoint {
            block_name: "b",
            time_var: "tau",
            t0: 0.0,
            states: &[],
            x0: &[],
            analytic_values: &analytic,
        };
        let pinned = op.pinned(3.0, &[], None);
        assert_eq!(pinned["tau"], 3.0);
        assert_eq!(pinned["time"], 99.0);
    }

    // ── injection ───────────────────────────────────────────────────────────

    #[test]
    fn emit_matrix_writes_two_d_entries_and_registers_display_names() {
        let mut out = Vec::new();
        let mut display = BTreeMap::new();
        emit_matrix(
            &mut out,
            &mut display,
            "A",
            &[vec![-0.004, 0.004], vec![0.0025, -0.0034375]],
        );
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].lhs, Expr::Var("a[1,1]".into()));
        assert_eq!(out[0].rhs, Expr::num(-0.004));
        assert_eq!(out[3].lhs, Expr::Var("a[2,2]".into()));
        assert_eq!(display["a[1,2]"], "A[1,2]");
        assert_eq!(display["a[2,1]"], "A[2,1]");
        assert!(!display.contains_key("a[1]"), "no 1-D form for 2 columns");
        assert_eq!(out[1].source_text, "a[1,2] (linearized)");
    }

    #[test]
    fn a_single_column_matrix_also_gets_the_one_d_form() {
        let mut out = Vec::new();
        let mut display = BTreeMap::new();
        emit_matrix(&mut out, &mut display, "B", &[vec![2.0e-4], vec![9.0e-4]]);
        // Two rows × (2-D + 1-D).
        assert_eq!(out.len(), 4);
        let names: Vec<_> = out
            .iter()
            .map(|e| match &e.lhs {
                Expr::Var(v) => v.clone(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(names, vec!["b[1,1]", "b[1]", "b[2,1]", "b[2]"]);
        assert_eq!(display["b[1]"], "B[1]");
        assert_eq!(display["b[2,1]"], "B[2,1]");
    }

    #[test]
    fn emit_matrix_does_not_clobber_an_existing_display_name() {
        let mut out = Vec::new();
        let mut display = BTreeMap::new();
        display.insert("a[1,1]".to_string(), "Amatrix[1,1]".to_string());
        emit_matrix(&mut out, &mut display, "A", &[vec![1.0]]);
        assert_eq!(display["a[1,1]"], "Amatrix[1,1]", "putIfAbsent semantics");
    }

    #[test]
    fn an_empty_matrix_emits_nothing() {
        let mut out = Vec::new();
        let mut display = BTreeMap::new();
        emit_matrix(&mut out, &mut display, "D", &[]);
        emit_matrix(&mut out, &mut display, "D", &[vec![], vec![]]);
        assert!(out.is_empty());
        assert!(display.is_empty());
    }

    #[test]
    fn scalar_constants_are_extracted_and_complex_literals_skipped() {
        let equations = vec![
            Equation::new(Expr::var("Q_in"), Expr::num(1000.0), "Q_in = 1000"),
            Equation::new(
                Expr::var("z"),
                Expr::Num {
                    value: 2.0,
                    unit: None,
                    is_imaginary: true,
                },
                "z = 2i",
            ),
            Equation::new(
                Expr::var("y"),
                Expr::bin(BinOp::Add, Expr::num(1.0), Expr::num(2.0)),
                "y = 1 + 2",
            ),
        ];
        let c = extract_scalar_constants(&equations);
        assert_eq!(c.get("q_in"), Some(&1000.0));
        assert!(!c.contains_key("z"), "an imaginary literal is not a value");
        assert!(!c.contains_key("y"), "only a bare literal counts");
    }

    #[test]
    fn display_names_invert_and_resolve_dotted_headers() {
        let mut display = BTreeMap::new();
        display.insert("m$port$t".to_string(), "M.port.T".to_string());
        display.insert("q_in".to_string(), "Q_in".to_string());
        let inv = invert_display_names(&display);
        assert_eq!(inv["m.port.t"], "m$port$t");
        assert_eq!(inv["q_in"], "q_in");
        // The header spelling is already lowercased by the parser.
        let resolved = resolve_names(&["m.port.t".to_string(), "unknown.thing".to_string()], &inv);
        assert_eq!(resolved, vec!["m$port$t", "unknown.thing"]);
    }
}
