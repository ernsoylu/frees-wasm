//! End-to-end tests for the Phase-4 evaluator dispatch that was wired last:
//! `proc$<name>$<k>` procedure-output synthetics (which carry both `CALL` of a
//! `PROCEDURE` and the destructuring `[a, b] = f(x)` form), the two quadrature
//! intrinsics, and the vector-argument kernels reached by name.
//!
//! These drive only the public API (`frees_core::solve`), so they exercise the
//! whole pipeline the CLI and the wasm boundary use: parse → flatten CALLs →
//! expand → block → Newton, with residuals evaluated through
//! `eval_with`/`EvalContext`.

use frees_core::{solve, FreesError, Solution, SolverSettings};

fn solved(source: &str) -> Solution {
    solve(source, &SolverSettings::default())
        .unwrap_or_else(|err| panic!("expected {source:?} to solve, got: {err}"))
}

fn failed(source: &str) -> FreesError {
    match solve(source, &SolverSettings::default()) {
        Ok(solution) => panic!("expected {source:?} to fail, got {:?}", solution.values),
        Err(failure) => failure.error,
    }
}

fn get(solution: &Solution, name: &str) -> f64 {
    *solution
        .values
        .get(name)
        .unwrap_or_else(|| panic!("no value for `{name}`; have {:?}", solution.values))
}

#[track_caller]
fn assert_near(actual: f64, expected: f64) {
    let tolerance = (1e-9 * expected.abs()).max(1e-12);
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected}, got {actual}"
    );
}

// ---------------------------------------------------------------------------
// CALL of a PROCEDURE
// ---------------------------------------------------------------------------

#[test]
fn call_of_a_procedure_binds_every_output() {
    let solution =
        solved("PROCEDURE p(a : b, c)\n  b := a * 2\n  c := a + 1\nEND\nCALL p(3 : y, z)\n");
    assert_eq!(get(&solution, "y"), 6.0);
    assert_eq!(get(&solution, "z"), 4.0);
}

#[test]
fn a_procedure_input_may_be_an_expression_over_solved_variables() {
    let solution = solved(
        "PROCEDURE p(a : b, c)\n  b := a * 2\n  c := a + 1\nEND\n\
         k = 4\n\
         CALL p(k + 1 : y, z)\n",
    );
    assert_eq!(get(&solution, "y"), 10.0);
    assert_eq!(get(&solution, "z"), 6.0);
}

#[test]
fn a_procedure_output_feeds_the_rest_of_the_system() {
    // `w` depends on `y`, which only the CALL determines — the blocker has to
    // order the synthetic call ahead of the ordinary equation.
    let solution = solved(
        "PROCEDURE p(a : b, c)\n  b := a * 2\n  c := a + 1\nEND\n\
         CALL p(3 : y, z)\n\
         w = y + z\n",
    );
    assert_eq!(get(&solution, "w"), 10.0);
}

#[test]
fn a_procedure_body_may_loop_and_branch() {
    let solution = solved(
        "PROCEDURE acc(n : total, big)\n\
         \x20 total := 0\n\
         \x20 i := 1\n\
         \x20 REPEAT\n\
         \x20   total := total + i\n\
         \x20   i := i + 1\n\
         \x20 UNTIL i > n\n\
         \x20 IF total > 10 THEN\n\
         \x20   big := 1\n\
         \x20 ELSE\n\
         \x20   big := 0\n\
         \x20 END\n\
         END\n\
         CALL acc(5 : s, flag)\n",
    );
    assert_eq!(get(&solution, "s"), 15.0);
    assert_eq!(get(&solution, "flag"), 1.0);
}

#[test]
fn a_procedure_body_may_call_a_user_function() {
    let solution = solved(
        "FUNCTION Double(x)\n  Double := 2 * x\nEND\n\
         PROCEDURE p(a : b, c)\n  b := Double(a)\n  c := Double(b)\nEND\n\
         CALL p(3 : y, z)\n",
    );
    assert_eq!(get(&solution, "y"), 6.0);
    assert_eq!(get(&solution, "z"), 12.0);
}

#[test]
fn a_never_assigned_output_is_named_not_silently_nan() {
    let message =
        failed("PROCEDURE p(a : b, c)\n  b := a\nEND\nCALL p(1 : y, z)\n").to_string_message();
    assert!(
        message.contains("never assigned output variable 'c'"),
        "{message}"
    );
}

#[test]
fn a_runaway_while_in_a_procedure_is_refused_not_hung() {
    let message = failed(
        "PROCEDURE spin(seed : out)\n\
         \x20 out := seed\n\
         \x20 WHILE out > 0 DO\n\
         \x20   out := out + 1\n\
         \x20 END\n\
         END\n\
         CALL spin(1 : v)\n",
    )
    .to_string_message();
    assert!(message.contains("WHILE loop exceeded"), "{message}");
}

// ---------------------------------------------------------------------------
// Multi-output FUNCTION destructuring
// ---------------------------------------------------------------------------

#[test]
fn a_multi_output_function_destructures_into_both_targets() {
    let solution =
        solved("FUNCTION [p, q] = two(u)\n  p := u\n  q := u * 2\nEND\n[g, h] = two(4)\n");
    assert_eq!(get(&solution, "g"), 4.0);
    assert_eq!(get(&solution, "h"), 8.0);
}

#[test]
fn a_multi_output_function_also_answers_the_explicit_call_form() {
    // `FUNCTION [a, b] = f(x)` desugars to a PROCEDURE, so `CALL` reaches it.
    let solution =
        solved("FUNCTION [p, q] = two(u)\n  p := u\n  q := u * 2\nEND\nCALL two(4 : g, h)\n");
    assert_eq!(get(&solution, "g"), 4.0);
    assert_eq!(get(&solution, "h"), 8.0);
}

#[test]
fn a_discarded_output_slot_does_not_surface_as_a_variable() {
    let solution =
        solved("FUNCTION [p, q] = two(u)\n  p := u\n  q := u * 2\nEND\n[~, h] = two(4)\n");
    assert_eq!(get(&solution, "h"), 8.0);
    assert!(
        !solution.values.keys().any(|k| k == "p" || k == "g"),
        "unexpected surfaced outputs: {:?}",
        solution.values.keys().collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Vector-argument kernels reached by name
// ---------------------------------------------------------------------------

#[test]
fn interp2_evaluates_in_expression_position_in_both_orders() {
    // Z[i][j] = x[i] + y[j] on the unit square; the query sits on a corner.
    let call_order = solved("z = Interp2([0, 1], [0, 1], [0, 1; 1, 2], 1, 1)\n");
    assert_near(get(&call_order, "z"), 2.0);
    let query_first = solved("z = Interp2(1, 1, [0, 1], [0, 1], [[0, 1], [1, 2]])\n");
    assert_near(get(&query_first, "z"), 2.0);
    // Interior point, bilinear on a 2x2 grid.
    let midpoint = solved("z = Interp2([0, 1], [0, 1], [0, 1; 1, 2], 0.25, 0.5)\n");
    assert_near(get(&midpoint, "z"), 0.75);
}

#[test]
fn lin_fit_names_solve_end_to_end() {
    // (1,2),(2,3),(3,5): slope 1.5, intercept 1/3, R^2 = 27/28.
    let solution = solved(
        "m = slope([1, 2, 3], [2, 3, 5])\n\
         b = intercept([1, 2, 3], [2, 3, 5])\n\
         rr = r2([1, 2, 3], [2, 3, 5])\n",
    );
    assert_near(get(&solution, "m"), 1.5);
    assert_near(get(&solution, "b"), 1.0 / 3.0);
    assert_near(get(&solution, "rr"), 27.0 / 28.0);
}

// ---------------------------------------------------------------------------
// Quadrature dispatch
// ---------------------------------------------------------------------------

#[test]
fn gauss_integral_reaches_the_quadrature_kernel() {
    // The oracle's value for ∫₀¹ t² dt. `crate::integral` is a frozen contract
    // owned by another agent: until its body lands the engine must refuse with
    // the *kernel's* message, never with "not yet supported: gaussintegral",
    // which would mean the dispatch is still missing.
    match solve(
        "G = GaussIntegral(t^2, t, 0, 1)\n",
        &SolverSettings::default(),
    ) {
        Ok(solution) => assert_near(get(&solution, "g"), 0.3333333333333333),
        Err(failure) => {
            let message = failure.error.to_string_message();
            assert!(
                message.contains("GaussIntegral is not yet supported"),
                "dispatch regressed: {message}"
            );
        }
    }
}

#[test]
fn an_integral_over_a_degenerate_interval_is_zero_without_the_kernel() {
    // `Evaluator.integralQuadrature` short-circuits `lower == upper` before it
    // touches the quadrature, so this holds regardless of the kernel's state.
    let solution = solved("G = GaussIntegral(t^2, t, 2, 2)\n");
    assert_eq!(get(&solution, "g"), 0.0);
}

#[test]
fn gauss_integral_does_not_leak_its_integration_variable_as_an_unknown() {
    // `Expr::variables` binds `t` inside the integrand only; if it escaped, the
    // system would be one equation short and the blocker would say so.
    let solution = solved("G = GaussIntegral(t^2, t, 2, 2)\n");
    assert!(
        !solution.values.keys().any(|k| k == "t"),
        "`t` escaped as an unknown: {:?}",
        solution.values.keys().collect::<Vec<_>>()
    );
}
