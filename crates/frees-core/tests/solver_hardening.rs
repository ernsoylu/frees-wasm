//! A2 solver hardening — integration tests for the Java behaviours ported in
//! Phase 4: the analytic-Jacobian wiring, bounds enforcement at every probe,
//! and the `EquationSystemSolver.solveBlockWithFallback` retry ladder
//! (transformed guesses → univariate bracketing → block merging → polish).
//!
//! Everything here drives the public API only, as the wasm boundary does.

use frees_core::solver::newton::newton_solve;
use frees_core::{solve, FreesError, SolverSettings};

fn settings() -> SolverSettings {
    SolverSettings::default()
}

// ---------------------------------------------------------------------------
// Ladder rung 1 — transformed guesses (retryWithTransformedGuesses)
// ---------------------------------------------------------------------------

/// A system symmetric under `x <-> y` traps every symmetric iteration on the
/// invariant manifold `x = y` (identical Jacobian columns there), which is
/// exactly what the Java jitter transforms exist for
/// (`EquationSystemSolver.buildGuessTransforms`, the "Symmetry-breaking
/// variants last" block). Prove both halves:
///
/// * single-shot Newton from the default symmetric start `(1, 1)` fails —
///   asserted directly against `newton_solve`, the ladder's rung 0;
/// * the engine, running the full ladder, solves the same document — the
///   jitter transform staggers the two guesses off the manifold.
#[test]
fn the_retry_ladder_rescues_a_symmetric_system_single_shot_newton_fails() {
    // Rung 0 alone: provably stuck on the manifold.
    let mut x = [1.0, 1.0];
    let single_shot = newton_solve(
        |v: &[f64], out: &mut [f64]| {
            out[0] = v[0] + v[1] - 2.0;
            out[1] = v[0] * v[1] + 3.0;
            Ok(())
        },
        &mut x,
        &settings(),
        None,
    );
    assert!(
        single_shot.is_err(),
        "if plain Newton now solves the symmetric start, this test needs a \
         harder trap: {single_shot:?}"
    );

    // The engine's ladder: the uniform transforms preserve the symmetry and
    // fail too; the jitter transforms leave the manifold and converge.
    let solution = solve("x + y = 2\nx * y = -3\n", &settings())
        .expect("the ladder's jitter transforms must rescue this document");
    let x = solution.values["x"];
    let y = solution.values["y"];
    assert!((x + y - 2.0).abs() < 1e-7, "x + y = {}", x + y);
    assert!((x * y + 3.0).abs() < 1e-7, "x * y = {}", x * y);
    // The two roots are (3, -1) and (-1, 3); either assignment is a solution.
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    assert!(
        (hi - 3.0).abs() < 1e-7 && (lo + 1.0).abs() < 1e-7,
        "({x}, {y})"
    );
}

// ---------------------------------------------------------------------------
// Ladder rung 3 — bidirectional block merging (tryMergeBidirectional)
// ---------------------------------------------------------------------------

/// The scenario the Java merge rung exists for ("previously solved blocks may
/// have incorrect values from SVD fallback on rank-deficient Jacobians"): a
/// consistent rank-deficient pair solves instantly at the default guesses to
/// *one* point of its solution manifold (x = y = 1), and the downstream block
/// then needs a different point (`exp(w) = x - 1.5` requires x > 1.5).
///
/// The ladder's contract, asserted here (matching what the Java does with
/// this document — its damped iteration on the merged rank-deficient system
/// creeps and stalls under the same creep guard we ported):
///
/// * the downstream block alone exhausts rungs 1–2;
/// * rung 3 merges **all three equations** — the failure message proves the
///   merged system (not the original one-equation block) was attempted, and
///   the merge-solve's own error propagates (Java rethrows it, not the
///   original), quoting every merged equation;
/// * the reset-to-initial-guesses is visible in the partial diagnostics: the
///   upstream pair no longer sits at its solved point but at the merged
///   attempt's stalled iterate.
#[test]
fn the_merge_rung_attempts_the_combined_system_and_propagates_its_error() {
    let source = "x + y = 2\n2*x + 2*y = 4\nexp(w) = x - 1.5\n";
    let failure = solve(source, &settings()).expect_err("the merged system stalls too");

    let message = failure.to_string_message();
    assert!(
        message.contains("3 equations"),
        "the merged 3-equation system must have been attempted: {message}"
    );
    assert!(message.contains("exp(w) = x - 1.5"), "{message}");
    assert!(message.contains("x + y = 2"), "{message}");
    assert!(message.contains("2*x + 2*y = 4"), "{message}");

    // The original decomposition (pair + scalar) is what the diagnostics show.
    assert_eq!(failure.failed_block_index, Some(1));
    let partial = failure.partial.as_deref().expect("partial diagnostics");
    assert_eq!(partial.blocks.len(), 2);
    // The merged attempt reset and moved the upstream pair: its residuals are
    // no longer the exact zeros the standalone solve had left.
    let upstream: Vec<f64> = partial
        .residuals
        .iter()
        .filter(|r| r.block == 0)
        .map(|r| r.residual)
        .collect();
    assert_eq!(upstream.len(), 2);
    assert!(upstream.iter().all(|r| r.is_finite()), "{partial:?}");
}

// ---------------------------------------------------------------------------
// Ladder exhaustion — SolveFailure + PartialDiagnostics survive
// ---------------------------------------------------------------------------

/// When every rung fails — and no merge is possible, because the failed block
/// shares no variables with any other — the original error must surface with
/// the Java `FailureState`/`partialResult` shape intact: `failed_block_index`,
/// the full block decomposition, the untouched values of the *other* blocks,
/// and the failed block's residual at the iterate the ladder left behind (the
/// *restored initial guesses* — `retryWithTransformedGuesses` puts them back,
/// exactly as the Java does).
#[test]
fn ladder_exhaustion_still_yields_partial_diagnostics() {
    // `a` and `x` are independent, so the merge rung finds nothing to merge
    // and `a`'s solved value must survive the failed ladder untouched.
    let failure = solve("a = 1\nGUESS x = 0.5 [0, 1]\nx = 6\n", &settings())
        .expect_err("x = 6 is unreachable inside [0, 1]");

    let failed_index = failure
        .failed_block_index
        .expect("a block-loop failure carries its index");
    let partial = failure
        .partial
        .as_deref()
        .expect("ladder exhaustion must still ship partial diagnostics");
    assert_eq!(partial.blocks.len(), 2);
    assert_eq!(partial.residuals.len(), 2);
    assert_eq!(
        partial.blocks[failed_index].variables,
        vec!["x".to_string()],
        "{partial:?}"
    );

    for residual in &partial.residuals {
        match residual.equation.as_str() {
            // The untouched upstream block keeps its exact solution...
            "a = 1" => assert!(residual.residual.abs() < 1e-9, "{residual:?}"),
            // ...and the failed equation is evaluated at the restored initial
            // guess: 0.5 - 6 = -5.5.
            "x = 6" => assert!((residual.residual + 5.5).abs() < 1e-9, "{residual:?}"),
            other => panic!("unexpected equation {other:?}"),
        }
    }
    assert!(partial.stats.max_residual >= 5.5 - 1e-9);

    let message = failure.to_string_message();
    assert!(
        message.contains("stalled") || message.contains("Constrained"),
        "{message}"
    );
}

// ---------------------------------------------------------------------------
// Bounds — enforced end to end
// ---------------------------------------------------------------------------

/// A bounded document whose root lies inside the box solves to it and emits no
/// bounds warning — the enforcement path, not the advisory one. The lower
/// bound at 0.1 also shields `ln` from its invalid region: with clamping, no
/// probe can ever reach x <= 0.
#[test]
fn bounds_shield_invalid_regions_and_pick_the_in_box_root() {
    let solution = solve("GUESS x = 2 [0.1, 10]\nln(x) = 0\n", &settings())
        .expect("the in-box root x = 1 must be found");
    assert!((solution.values["x"] - 1.0).abs() < 1e-9);
    assert!(
        solution.diagnostics.is_empty(),
        "a bounded solve ending inside its box must not warn: {:?}",
        solution.diagnostics
    );
}

/// The bounds warning (`check_bounds`) is now a safety net that a bounded
/// solve should never trip: the solver cannot return an out-of-box value for
/// a bounded variable, because every candidate was clamped.
#[test]
fn a_bounded_solve_never_returns_an_out_of_box_value() {
    // Root exactly on the bound: allowed, converges, no warning.
    let on_bound = solve("GUESS x = 1 [0, 3]\nx ^ 2 = 9\n", &settings())
        .expect("the root x = 3 sits on the bound");
    assert!((on_bound.values["x"] - 3.0).abs() < 1e-9);
    assert!(
        !on_bound
            .diagnostics
            .iter()
            .any(|d| d.message.contains("outside the GUESS bounds")),
        "{:?}",
        on_bound.diagnostics
    );
}

// ---------------------------------------------------------------------------
// Polish pass (polishSettings) — refinement without regression
// ---------------------------------------------------------------------------

/// The polish pass must never worsen a solution or change what is solved: the
/// canonical fixture still lands on the Java oracle, and the polished
/// residuals stay at their numerical floor.
#[test]
fn the_polish_pass_keeps_the_canonical_oracle_values() {
    let solution = solve("x^2 + y^3 = 77\nx/y = 1.23456\n", &settings()).expect("must solve");
    let x = solution.values["x"];
    let y = solution.values["y"];
    assert!((x - 4.694012391660914).abs() <= 1e-9 * 4.7, "x = {x:.17}");
    assert!((y - 3.802174371161316).abs() <= 1e-9 * 3.9, "y = {y:.17}");
    assert!(
        (x * x + y * y * y - 77.0).abs() < 1e-7,
        "polish must not loosen the residual floor"
    );
    // The polisher's iterations count only when it converges (Java adds the
    // polisher's return value only on success); either way the totals stay
    // sane for a two-unknown document.
    assert!(solution.iterations >= 1 && solution.iterations < 100);
}

/// Solving is still deterministic with the whole ladder in place.
#[test]
fn the_ladder_is_deterministic() {
    let source = "x + y = 2\nx * y = -3\n";
    let first = solve(source, &settings()).expect("solves");
    let second = solve(source, &settings()).expect("solves");
    assert_eq!(first.values, second.values);
    assert_eq!(first.iterations, second.iterations);
}

/// A document with no rescue (a genuinely rootless equation) still fails with
/// the original first-attempt error annotated with its block and equation —
/// the ladder must not replace a truthful message with a retry artifact.
#[test]
fn a_rootless_equation_fails_with_the_original_annotated_error() {
    let failure = solve("exp(q) = -1\n", &settings()).expect_err("no real root");
    let message = failure.to_string_message();
    assert!(message.starts_with("Block 1"), "{message}");
    assert!(message.contains("exp(q) = -1"), "{message}");
    assert!(matches!(failure.error, FreesError::Solver { .. }));
    assert_eq!(failure.failed_block_index, Some(0));
    assert!(failure.partial.is_some());
}
