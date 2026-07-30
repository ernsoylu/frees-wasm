//! Round-trip tests for complex expansion: parse → `expand_complex` → block →
//! Newton, mirroring the complex-mode tests in the Java
//! `EquationSystemSolverTest` (`complexSolving`, `complexLiterals`,
//! `complexAbsIsMagnitude`, `complexRealAndImag`,
//! `complexUnsupportedFunctionIsRejected`).
//!
//! The pipeline here reproduces what `EquationSystemSolver.solveEquationList`
//! does after expansion, including the Java `expandSpecs` seeding rule
//! (verified at `EquationSystemSolver.complexComponentSpec`): an `_r`
//! component starts at the base guess (default 1.0), an `_i` component
//! defaults to **0.0**, and an explicit spec for the suffixed name wins.

use std::collections::{HashMap, HashSet};

use frees_core::ast::Equation;
use frees_core::diag::Result;
// `frees_core::eval::eval` is the engine's pure AST interpreter over parsed
// `Expr` trees — no code execution of any kind.
use frees_core::eval::eval;
use frees_core::parser::complex::expand_complex;
use frees_core::parser::parse_document;
use frees_core::solver::blocker::block_system;
use frees_core::solver::newton::{newton_solve, SolverSettings};

/// Java `EquationSystemSolver.DEFAULT_GUESS`.
const DEFAULT_GUESS: f64 = 1.0;

/// Parse `source`, expand it in complex mode, and solve block by block.
/// `overrides` play the role of the Variable Information window (explicit
/// guesses for expanded names); in-text `GUESS` directives are honoured the
/// way `withTextGuesses` merges them — text wins over the override.
fn solve_complex(source: &str, overrides: &[(&str, f64)]) -> HashMap<String, f64> {
    let doc = parse_document(source).expect("parse");
    let equations: Vec<Equation> = doc.equations().into_iter().cloned().collect();
    let expanded = expand_complex(equations, true).expect("expand");
    let report = block_system(&expanded, &HashSet::new()).expect("block");

    // Seed: _r components at DEFAULT_GUESS, _i components at 0.0 (the Java
    // complexComponentSpec rule), then overrides, then in-text GUESS.
    let mut values: HashMap<String, f64> = HashMap::new();
    for eq in &expanded {
        for v in eq.variables() {
            let seed = if v.ends_with("_i") {
                0.0
            } else {
                DEFAULT_GUESS
            };
            values.entry(v).or_insert(seed);
        }
    }
    for (name, guess) in overrides {
        values.insert((*name).to_string(), *guess);
    }
    for g in &doc.guesses {
        if let Some(guess) = g.guess {
            values.insert(g.name.clone(), guess);
        }
    }

    for block in &report.blocks {
        let vars = &block.variables;
        let mut x: Vec<f64> = vars.iter().map(|v| values[v]).collect();
        let residual = |xs: &[f64], out: &mut [f64]| -> Result<()> {
            let mut scope = values.clone();
            for (v, val) in vars.iter().zip(xs) {
                scope.insert(v.clone(), *val);
            }
            for (row, &ei) in block.equations.iter().enumerate() {
                let eq = &expanded[ei];
                out[row] = eval(&eq.lhs, &scope)? - eval(&eq.rhs, &scope)?;
            }
            Ok(())
        };
        // `None` bounds: this harness solves the split system directly, with no
        // per-variable ranges (the engine supplies them from VariableSpec).
        newton_solve(residual, &mut x, &SolverSettings::default(), None)
            .unwrap_or_else(|e| panic!("newton failed on block {:?}: {e}", block.variables));
        for (v, val) in vars.iter().zip(&x) {
            values.insert(v.clone(), *val);
        }
    }
    values
}

fn assert_close(values: &HashMap<String, f64>, name: &str, expected: f64, tol: f64) {
    let got = values
        .get(name)
        .unwrap_or_else(|| panic!("{name} missing from {values:?}"));
    assert!(
        (got - expected).abs() < tol,
        "{name}: got {got}, want {expected}"
    );
}

// ── ports of the Java complex-mode solver tests ─────────────────────────────

/// Java `complexLiterals`: literals split, and multiplying by `1j` rotates.
#[test]
fn complex_literals_solve_to_their_parts() {
    let values = solve_complex("z = 3 + 4i\nw = 1j * z", &[]);
    assert_close(&values, "z_r", 3.0, 1e-6);
    assert_close(&values, "z_i", 4.0, 1e-6);
    assert_close(&values, "w_r", -4.0, 1e-6);
    assert_close(&values, "w_i", 3.0, 1e-6);
}

/// Java `complexSolving`: z² = −4 lands on ±2i. The Java default seed
/// (z_i = 0) sits exactly on the real axis where no real root exists; Java
/// escapes through its solve-retry ladder, which this port does not have yet
/// (known divergence #2 in `docs/status-phase1.md`), so the test steers the
/// imaginary component off the axis the way the Variable Information window
/// would.
#[test]
fn z_squared_equals_minus_four_yields_pure_imaginary_roots() {
    let values = solve_complex("z^2 = -4", &[("z_i", 1.0)]);
    assert_close(&values, "z_r", 0.0, 1e-6);
    let z_i = values["z_i"];
    assert!(
        (z_i - 2.0).abs() < 1e-6 || (z_i + 2.0).abs() < 1e-6,
        "z_i: got {z_i}, want ±2"
    );
}

/// Java `complexAbsIsMagnitude`: sqrt(−16) = ±4i so z = 3±4i and |z| = 5.
#[test]
fn abs_of_a_complex_value_is_its_magnitude() {
    let values = solve_complex("z = 3 + sqrt(-16)\nm = abs(z)", &[]);
    assert_close(&values, "z_r", 3.0, 1e-6);
    assert_close(&values, "m_r", 5.0, 1e-6);
    assert_close(&values, "m_i", 0.0, 1e-9);
    let z_i = values["z_i"].abs();
    assert!((z_i - 4.0).abs() < 1e-6, "|z_i|: got {z_i}, want 4");
}

/// Java `complexRealAndImag`: real()/imag() project onto the components.
#[test]
fn real_and_imag_project_the_components() {
    let values = solve_complex("z = 3 + 4i\na = real(z)\nb = imag(z)", &[]);
    assert_close(&values, "z_r", 3.0, 1e-6);
    assert_close(&values, "z_i", 4.0, 1e-6);
    assert_close(&values, "a_r", 3.0, 1e-6);
    assert_close(&values, "a_i", 0.0, 1e-9);
    assert_close(&values, "b_r", 4.0, 1e-6);
    assert_close(&values, "b_i", 0.0, 1e-9);
}

// ── the round-trips the phase-4 brief asks for ──────────────────────────────

/// (3+4i)(3−4i) = 9 + 16 = 25 through the split system.
#[test]
fn product_of_conjugate_literals_round_trips_to_25() {
    let values = solve_complex("p = (3 + 4i) * (3 - 4i)", &[]);
    assert_close(&values, "p_r", 25.0, 1e-9);
    assert_close(&values, "p_i", 0.0, 1e-9);
}

/// x² = −1 lands on +i or −i depending on which side the in-text GUESS
/// steers the imaginary component to.
#[test]
fn x_squared_equals_minus_one_follows_the_guess_steering() {
    let plus = solve_complex("x^2 = -1\nGUESS x_i = 1", &[]);
    assert_close(&plus, "x_r", 0.0, 1e-6);
    assert_close(&plus, "x_i", 1.0, 1e-6);

    let minus = solve_complex("x^2 = -1\nGUESS x_i = -1", &[]);
    assert_close(&minus, "x_r", 0.0, 1e-6);
    assert_close(&minus, "x_i", -1.0, 1e-6);
}

/// A division round-trip: (25 + 0i) / (3 + 4i) = 3 − 4i.
#[test]
fn division_by_a_complex_literal_round_trips() {
    let values = solve_complex("q = 25 / (3 + 4i)", &[]);
    assert_close(&values, "q_r", 3.0, 1e-9);
    assert_close(&values, "q_i", -4.0, 1e-9);
}

/// A purely real document still solves in complex mode: every imaginary part
/// comes out zero (directly or via the default pins).
#[test]
fn purely_real_document_solves_in_complex_mode_with_zero_imaginary_parts() {
    let values = solve_complex("x + y = 3\ny = x + 1", &[]);
    assert_close(&values, "x_r", 1.0, 1e-9);
    assert_close(&values, "y_r", 2.0, 1e-9);
    assert_close(&values, "x_i", 0.0, 1e-9);
    assert_close(&values, "y_i", 0.0, 1e-9);
}

/// Exp/ln round-trip: w = ln(z) for z = 3+4i, then back through exp.
#[test]
fn ln_and_exp_round_trip_through_the_polar_forms() {
    let values = solve_complex("z = 3 + 4i\nw = ln(z)\nv = exp(w)", &[]);
    assert_close(&values, "w_r", 5.0f64.ln(), 1e-9);
    assert_close(&values, "w_i", (4.0f64).atan2(3.0), 1e-9);
    assert_close(&values, "v_r", 3.0, 1e-8);
    assert_close(&values, "v_i", 4.0, 1e-8);
}

/// The mode gate end to end: the same imaginary document refuses to expand in
/// real mode with the Java guidance message.
#[test]
fn real_mode_refuses_imaginary_documents() {
    let doc = parse_document("z = 3 + 4i").expect("parse");
    let equations: Vec<Equation> = doc.equations().into_iter().cloned().collect();
    let err = expand_complex(equations, false).unwrap_err();
    assert!(
        err.to_string().contains("enable Complex mode"),
        "unexpected error: {err}"
    );
}
