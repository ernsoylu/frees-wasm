//! End-to-end integration tests: real `.frees` documents in, values out.
//!
//! These drive only the public API (`frees_core::solve` / `frees_core::check`)
//! — no internal helpers — so they exercise exactly what the wasm boundary and
//! the CLI call. Where a case is also a parity fixture under `fixtures/`, the
//! expected numbers are the ones the Java oracle produced (see
//! `fixtures/README.md`), compared with the documented relative tolerance
//! rather than bit-equality.

use std::collections::BTreeMap;

use frees_core::diag::Severity;
use frees_core::{
    check, check_with, solve, solve_with, FreesError, Solution, SolverSettings, VariableOverride,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn message(err: &FreesError) -> String {
    err.to_string_message()
}

fn get(solution: &Solution, name: &str) -> f64 {
    *solution
        .values
        .get(name)
        .unwrap_or_else(|| panic!("no value for `{name}`; have {:?}", keys(&solution.values)))
}

fn keys(values: &BTreeMap<String, f64>) -> Vec<&str> {
    values.keys().map(String::as_str).collect()
}

/// `fixtures/README.md`: relative tolerance `1e-9`, absolute `1e-12` near zero.
#[track_caller]
fn assert_near(actual: f64, expected: f64) {
    let tolerance = (1e-9 * expected.abs()).max(1e-12);
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected}, got {actual} (delta {})",
        actual - expected
    );
}

/// The single unknown each block determines, in solve order. Panics on a
/// simultaneous block, which is the point — a test asserting an order should
/// not silently pass on a system that blocked differently than expected.
fn block_order(solution: &Solution) -> Vec<&str> {
    solution
        .blocks
        .iter()
        .map(|b| {
            assert_eq!(
                b.variables.len(),
                1,
                "expected only scalar blocks, got {:?}",
                b.variables
            );
            b.variables[0].as_str()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The canonical reference case
// ---------------------------------------------------------------------------

/// The pair named in `../frEES/CLAUDE.md`'s Continuous Verification rule, and
/// the `fixtures/corpus/canonical.frees` fixture. Expected values are the Java
/// oracle's, from `fixtures/golden/canonical.json`.
#[test]
fn canonical_two_by_two_matches_the_java_oracle() {
    let solution = solved(
        "{ The reference case named in ../frEES/CLAUDE.md's Continuous Verification rule. }\n\
         x^2 + y^3 = 77\n\
         x/y = 1.23456\n",
    );

    assert_near(get(&solution, "x"), 4.694_012_391_660_914);
    assert_near(get(&solution, "y"), 3.802_174_371_161_316);

    // Neither equation solves alone: this must be a single 2×2 block.
    assert_eq!(solution.blocks.len(), 1, "golden block_count is 1");
    assert_eq!(solution.blocks[0].equations.len(), 2);
    assert_eq!(solution.blocks[0].variables, vec!["x", "y"]);
    assert!(!solution.blocks[0].is_scalar());
    assert!(solution.iterations > 1);
    assert!(
        solution.diagnostics.is_empty(),
        "{:?}",
        solution.diagnostics
    );
}

/// The residuals of the canonical solution really are zero — the values above
/// are a solution, not just a stable point the solver liked.
#[test]
fn canonical_residuals_vanish_at_the_reported_values() {
    let solution = solved("x^2 + y^3 = 77\nx/y = 1.23456\n");
    let x = get(&solution, "x");
    let y = get(&solution, "y");
    // The Jacobian is built by finite differences with `jacobian_epsilon`
    // (1e-7 by default), which caps the achievable accuracy at roughly 1e-9
    // relative — the residuals vanish to that, not to machine epsilon.
    assert!(
        (x * x + y * y * y - 77.0).abs() < 1e-7,
        "{}",
        x * x + y * y * y - 77.0
    );
    assert!((x / y - 1.23456).abs() < 1e-9, "{}", x / y - 1.23456);
}

/// `check` sees the same system without touching the solver.
#[test]
fn canonical_passes_check_before_solve() {
    let report = check("x^2 + y^3 = 77\nx/y = 1.23456\n").unwrap();
    assert!(report.solvable);
    assert_eq!(report.equation_count, 2);
    assert_eq!(report.unknown_count, 2);
    assert_eq!(report.variables, vec!["x", "y"]);
    assert_eq!(
        report.message,
        "No syntax errors were detected. There are 2 equations and 2 variables."
    );
}

// ---------------------------------------------------------------------------
// Sequential chains — values AND block order
// ---------------------------------------------------------------------------

/// `fixtures/corpus/sequential.frees`: the document is written back to front on
/// purpose. frees is an equation solver, not a sequential language, so the
/// blocks must come out in *dependency* order.
#[test]
fn a_sequential_chain_solves_in_dependency_order() {
    let solution = solved("c = b + 1\nb = a * 3\na = 2\n");

    assert_near(get(&solution, "a"), 2.0);
    assert_near(get(&solution, "b"), 6.0);
    assert_near(get(&solution, "c"), 7.0);

    assert_eq!(solution.blocks.len(), 3, "golden block_count is 3");
    assert_eq!(block_order(&solution), vec!["a", "b", "c"]);
    // Source order was c, b, a — so the equation indices must run backwards.
    let equation_order: Vec<usize> = solution.blocks.iter().map(|b| b.equations[0]).collect();
    assert_eq!(equation_order, vec![2, 1, 0]);
}

/// The same chain written in the natural order must block identically —
/// blocking is a property of the system, not of the text.
#[test]
fn source_order_does_not_change_the_blocking() {
    let forwards = solved("a = 2\nb = a * 3\nc = b + 1\n");
    let backwards = solved("c = b + 1\nb = a * 3\na = 2\n");

    assert_eq!(block_order(&forwards), block_order(&backwards));
    assert_eq!(forwards.values, backwards.values);
}

/// `fixtures/corpus/mixed_blocks.frees`: sequential feed-in, a genuine 2×2, and
/// a sequential tail — three blocks, one of them simultaneous.
#[test]
fn a_mixed_document_produces_scalar_and_simultaneous_blocks() {
    let solution = solved("k = 4\np + q = k\np - q = 1\nr = p * q\n");

    assert_near(get(&solution, "k"), 4.0);
    assert_near(get(&solution, "p"), 2.5);
    assert_near(get(&solution, "q"), 1.5);
    assert_near(get(&solution, "r"), 3.75);

    assert_eq!(solution.blocks.len(), 3, "golden block_count is 3");
    assert_eq!(solution.blocks[0].variables, vec!["k"]);
    assert_eq!(solution.blocks[1].variables, vec!["p", "q"]);
    assert_eq!(solution.blocks[2].variables, vec!["r"]);
    assert!(solution.blocks[0].is_scalar());
    assert!(!solution.blocks[1].is_scalar());
}

/// Case-insensitivity is an engine invariant: `Tin`, `TIN` and `tin` are one
/// variable, keyed lowercase in the result.
#[test]
fn variable_names_are_case_insensitive_end_to_end() {
    let solution = solved("Tin = 300\nT_out = TIN * 2\nresult = t_Out + tin\n");

    assert_eq!(keys(&solution.values), vec!["result", "t_out", "tin"]);
    assert_near(get(&solution, "tin"), 300.0);
    assert_near(get(&solution, "t_out"), 600.0);
    assert_near(get(&solution, "result"), 900.0);
    assert_eq!(block_order(&solution), vec!["tin", "t_out", "result"]);
}

// ---------------------------------------------------------------------------
// Units convert to SI at parse time
// ---------------------------------------------------------------------------

/// `fixtures/corpus/units_pressure.frees`. All calculation is in SI: the
/// annotation is consumed by the parser and `140 [kPa]` reaches the solver as
/// `140000.0`. Nothing downstream ever sees kilopascals.
#[test]
fn a_unit_annotated_literal_is_si_by_the_time_it_is_solved() {
    let solution = solved("P = 140 [kPa]\nQ = P * 2\n");

    assert_near(get(&solution, "p"), 140_000.0);
    assert_near(get(&solution, "q"), 280_000.0);
    assert_eq!(solution.blocks.len(), 2, "golden block_count is 2");
    assert_eq!(block_order(&solution), vec!["p", "q"]);
}

/// `fixtures/corpus/units_temperature.frees`: temperature scales carry an
/// additive offset, and `F` also carries the 5/9 factor.
#[test]
fn temperature_scales_apply_their_offsets() {
    let solution = solved("T1 = 25 [C]\nT2 = 32 [F]\n");
    assert_near(get(&solution, "t1"), 298.15);
    assert_near(get(&solution, "t2"), 273.15);
}

/// `fixtures/corpus/units_negative_celsius.frees` — the unary-sign trap. The
/// minus folds into the literal *before* conversion, so this is 263.15 K and
/// not −283.15 K.
#[test]
fn a_negative_celsius_literal_folds_the_sign_before_converting() {
    let solution = solved("T = -10 [C]\n");
    assert_near(get(&solution, "t"), 263.15);
}

/// A whole mixed-unit document: unit conversion has to survive being an operand
/// of arbitrary arithmetic, not just a bare right-hand side.
#[test]
fn unit_conversion_survives_arithmetic_and_downstream_blocks() {
    let solution = solved(
        "P_in = 2 [bar]\n\
         dP = 50 [kPa]\n\
         P_out = P_in - dP\n\
         A = 10 [cm^2]\n\
         F = P_out * A\n",
    );

    assert_near(get(&solution, "p_in"), 200_000.0);
    assert_near(get(&solution, "dp"), 50_000.0);
    assert_near(get(&solution, "p_out"), 150_000.0);
    assert_near(get(&solution, "a"), 1e-3);
    assert_near(get(&solution, "f"), 150.0);
}

// ---------------------------------------------------------------------------
// GUESS directives
// ---------------------------------------------------------------------------

/// `fixtures/corpus/guess_directive.frees`: `x^2 = 9` has two real roots and the
/// default guess of 1.0 would find `+3`. The directive is what makes the
/// document deterministic.
#[test]
fn a_guess_directive_selects_the_root() {
    assert_near(get(&solved("GUESS x = 3\nx ^ 2 = 9\n"), "x"), 3.0);
    assert_near(get(&solved("GUESS x = -3\nx ^ 2 = 9\n"), "x"), -3.0);
}

/// A document that genuinely *needs* its GUESS: `tan(x) = 1` has a root every
/// π, and `x = 3.5` is on the branch containing `π + π/4 ≈ 3.927`. Without the
/// directive the default start of 1.0 lands on the principal root `π/4`, so the
/// two answers differ — which is exactly what "needs the guess" means.
#[test]
fn a_guess_directive_reaches_a_root_the_default_start_cannot() {
    let guided = solved("GUESS x = 3.5\ntan(x) = 1\n");
    assert_near(
        get(&guided, "x"),
        std::f64::consts::PI + std::f64::consts::FRAC_PI_4,
    );

    let unguided = solved("tan(x) = 1\n");
    assert_near(get(&unguided, "x"), std::f64::consts::FRAC_PI_4);
}

/// A steep exponential residual: from the default start of 1.0 the Newton step
/// is astronomically long, and the document supplies a guess in the right
/// neighbourhood. The solve must both converge and land on the analytic root.
#[test]
fn a_guess_directive_rescues_a_stiff_exponential() {
    let solution = solved("GUESS x = 10\nexp(x - 10) + x = 11\n");
    assert_near(get(&solution, "x"), 10.0);
    assert!(
        solution.diagnostics.is_empty(),
        "{:?}",
        solution.diagnostics
    );
}

/// Bounds are enforced during iteration (the Java `NewtonSolver` clamps every
/// line-search candidate, damped candidate and Jacobian probe into `[lo, hi]`):
/// they pick the root inside the box, and a document whose equations force a
/// value outside the box *fails* — through the whole retry ladder — rather
/// than quietly returning an out-of-range answer. (Formerly ranked divergence
/// #3 in `docs/status-phase1.md`: "Bounds are advisory". They no longer are.)
#[test]
fn guess_bounds_are_enforced_during_iteration() {
    // The bounds pick the negative root of x^2 = 9.
    let inside = solved("GUESS x = -2 [-5, 0]\nx ^ 2 = 9\n");
    assert_near(get(&inside, "x"), -3.0);
    assert!(inside.diagnostics.is_empty(), "{:?}", inside.diagnostics);

    // Here the equations force a value the bounds forbid: the Java engine
    // fails this document (the iterate can never leave [0, 1]), and so do we.
    let failure = solve("GUESS x = 0.5 [0, 1]\nx = 5\n", &SolverSettings::default())
        .expect_err("x = 5 is unreachable inside [0, 1]");
    assert_eq!(failure.failed_block_index, Some(0));
    assert!(
        failure.partial.is_some(),
        "ladder exhaustion must still carry partial diagnostics"
    );
    let message = failure.to_string_message();
    assert!(
        message.contains("stalled") || message.contains("Constrained"),
        "{message}"
    );
}

/// A GUESS naming nothing in the system is a warning, never a hard failure —
/// stale hints left behind after an edit must not break a working document.
#[test]
fn a_stale_guess_directive_only_warns() {
    let solution = solved("GUESS removed_variable = 3\nx = 1\n");
    assert_near(get(&solution, "x"), 1.0);
    assert_eq!(solution.diagnostics.len(), 1);
    assert_eq!(solution.diagnostics[0].severity, Severity::Warning);
    assert!(solution.diagnostics[0].message.contains("removed_variable"));
}

// ---------------------------------------------------------------------------
// Structural failures
// ---------------------------------------------------------------------------

/// `fixtures/corpus/overdetermined.frees`: two equations, one unknown. The
/// golden fixture's classification is `SolverException` and the message names
/// the redundant relation.
#[test]
fn an_over_determined_document_is_a_degrees_of_freedom_error() {
    let err = failed("z = 1\nz = 2\n");
    assert!(matches!(err, FreesError::Solver { .. }), "{err:?}");

    let text = message(&err);
    assert!(
        text.contains("There are 2 equations and 1 variables"),
        "{text}"
    );
    assert!(text.contains("overspecified"), "{text}");
    // The specific redundant relation is quoted, per the fixture README's
    // "assert the message names the same offending variables".
    assert!(text.contains("z=2") || text.contains("z = 2"), "{text}");
    // A DOF failure is structural, so it has no source span to point at.
    assert!(err.span().is_none());
}

/// A larger over-determined case: three equations for two unknowns.
#[test]
fn a_wider_over_determined_document_reports_the_real_counts() {
    let err = failed("a + b = 3\na - b = 1\na = 2\nb = 1\n");
    let text = message(&err);
    assert!(
        text.contains("There are 4 equations and 2 variables"),
        "{text}"
    );
    assert!(text.contains("overspecified"), "{text}");
}

/// `check` reports the same failure without solving, and without erroring —
/// the editor needs the counts to render the gutter.
#[test]
fn check_reports_a_degrees_of_freedom_failure_as_data() {
    let report = check("z = 1\nz = 2\n").unwrap();
    assert!(!report.solvable);
    assert_eq!(report.equation_count, 2);
    assert_eq!(report.unknown_count, 1);
    assert_eq!(report.variables, vec!["z"]);
    assert!(
        report.message.contains("overspecified"),
        "{}",
        report.message
    );
}

/// `fixtures/corpus/underdetermined.frees`: the mirror image.
#[test]
fn an_under_determined_document_names_the_free_quantity() {
    let err = failed("m + n = 5\n");
    let text = message(&err);
    assert!(
        text.contains("There are 1 equations and 2 variables"),
        "{text}"
    );
    assert!(text.contains("underspecified"), "{text}");
    assert!(text.contains("Free quantity"), "{text}");
    assert!(text.contains('n'), "{text}");
}

/// `fixtures/corpus/empty.frees`: a comment-only document is a failure, not a
/// success with nothing in it.
#[test]
fn a_comment_only_document_is_a_solver_error() {
    let err = failed("{ A document that is only a comment. }\n");
    assert_eq!(message(&err), "No equations to solve.");
}

/// Square but structurally singular: three equations, three unknowns, and yet
/// no complete assignment exists — `a` is pinned twice while `b` and `c` share
/// a single relation. The counts balance, so only the matching catches it.
#[test]
fn a_square_but_singular_system_is_refused() {
    let err = failed("a = 1\na = 2\nb + c = 3\n");
    let text = message(&err);
    assert!(matches!(err, FreesError::Solver { .. }), "{err:?}");
    assert!(text.contains("structurally singular"), "{text}");
    // Both sides are named: the redundant relation and the free quantity.
    assert!(text.contains("square"), "{text}");

    // `check` classifies it the same way without solving.
    let report = check("a = 1\na = 2\nb + c = 3\n").unwrap();
    assert!(!report.solvable);
    assert_eq!(report.equation_count, 3);
    assert_eq!(report.unknown_count, 3);
}

// ---------------------------------------------------------------------------
// Unsupported constructs
// ---------------------------------------------------------------------------

/// A wrong answer is worse than a refusal: every block form the port has not
/// reached is named explicitly rather than skipped.
#[test]
fn an_unsupported_component_block_is_refused_by_name() {
    let err = failed("COMPONENT pump\n  P_out = P_in * 2\nEND\n\nP_in = 1 [bar]\n");
    assert!(matches!(err, FreesError::Parse { .. }), "{err:?}");

    let text = message(&err);
    assert!(text.contains("COMPONENT"), "{text}");
    assert!(text.contains("not supported"), "{text}");
    // Source-mapped: it points at the offending construct.
    let span = err.span().expect("unsupported constructs carry a span");
    assert_eq!(span.start, 0);
    assert!(span.end > span.start);
}

#[test]
fn every_unported_block_form_is_refused_by_its_own_name() {
    // FUNCTION/PROCEDURE/MODULE/TABLE parse into `Document::defs` since the
    // Phase-4 procedural pass and are no longer refused here.
    for (source, construct) in [
        ("PLOT 'speed'\n  kind = xy\nEND\n", "PLOT"),
        ("DYNAMIC d(method = ode45)\n  der = 1\nEND\n", "DYNAMIC"),
        ("LINEARIZE plant(block = w)\n  INPUT q\nEND\n", "LINEARIZE"),
        ("PARAMETRIC table\nEND\n", "PARAMETRIC"),
        ("COMPONENT c\nEND\n", "COMPONENT"),
    ] {
        let err = failed(source);
        assert!(
            matches!(err, FreesError::Parse { .. }),
            "{construct}: {err:?}"
        );
        assert!(
            message(&err).contains(construct),
            "{construct} not named in: {}",
            message(&err)
        );
    }
}

/// The same refusal reaches `check`, so the Solve button is never enabled for a
/// document the engine cannot honour. Since the parse-failure rework, check
/// answers with the not-solvable report the Java 400-with-body carries rather
/// than an `Err` — the refusal is data the editor can render.
#[test]
fn check_refuses_an_unsupported_construct_too() {
    let report = check("COMPONENT pump\nEND\n").unwrap();
    assert!(!report.solvable);
    assert!(report.message.contains("COMPONENT"), "{}", report.message);
    assert!(
        report.message.starts_with("Syntax error: "),
        "{}",
        report.message
    );
    assert_eq!(report.error_line, Some(1));
}

/// `CALL` parses (it is a `Statement`, not a block) and flattens through the
/// Phase-4 procedure flattener. A CALL to a name no definition declares must
/// be refused with the Java `flattenCallProc` message, not silently dropped —
/// dropping it would change the degrees of freedom.
#[test]
fn a_call_statement_is_refused_rather_than_dropped() {
    let err = failed("CALL mix(1, 2 : y)\nx = y + 1\n");
    let text = message(&err);
    assert!(
        text.contains("Unknown PROCEDURE or MODULE: 'mix'"),
        "{text}"
    );

    // A CALL naming an intrinsic the port has not reached keeps the explicit
    // not-yet-supported refusal.
    let err = failed("[a, b] = tf2ss(num, den)\nnum = 1\nden = 2\n");
    let text = message(&err);
    assert!(text.contains("tf2ss"), "{text}");
    assert!(text.contains("not yet supported"), "{text}");
}

// ---------------------------------------------------------------------------
// Syntax errors
// ---------------------------------------------------------------------------

#[test]
fn syntax_errors_are_parse_errors_with_a_source_position() {
    for source in ["x = = 2\n", "x = (1 + 2\n", "x = 1 +\n", "= 5\n"] {
        let err = failed(source);
        assert!(
            matches!(err, FreesError::Parse { .. }),
            "{source:?}: {err:?}"
        );
        let span = err
            .span()
            .unwrap_or_else(|| panic!("{source:?} lost its span"));
        let (line, col) = span.line_col(source);
        assert!(line >= 1 && col >= 1);
    }
}

/// Unit problems are **warnings**, never hard failures — the parent engine's
/// invariant is that dimensional trouble is surfaced and the solve proceeds.
/// The literal keeps its face value (no conversion was possible) and the
/// document still solves, but the engine says so.
#[test]
fn an_unknown_unit_warns_and_still_solves() {
    let solution = solved("P = 140 [zorp]\nQ = P * 2\n");
    assert_near(get(&solution, "p"), 140.0);
    assert_near(get(&solution, "q"), 280.0);

    let warnings: Vec<&str> = solution
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("zorp"), "{warnings:?}");
    assert!(warnings[0].contains("not in SI"), "{warnings:?}");
    // The warning quotes the user's own line.
    assert_eq!(
        solution.diagnostics[0].source_text.as_deref(),
        Some("P = 140 [zorp]")
    );

    // `check` reports it too, without solving.
    let report = check("P = 140 [zorp]\n").unwrap();
    assert!(report.solvable);
    assert_eq!(report.diagnostics.len(), 1);
    assert!(report.diagnostics[0].message.contains("zorp"));
}

/// A known unit produces no warning at all — the check must not be noisy.
#[test]
fn known_units_produce_no_warnings() {
    let solution = solved("P = 140 [kPa]\nT = 25 [C]\nv = 3 [m/s]\nE = 2 [kJ/kg-K]\n");
    assert!(
        solution.diagnostics.is_empty(),
        "{:?}",
        solution.diagnostics
    );
}

// ---------------------------------------------------------------------------
// Intrinsics, constants and evaluation
// ---------------------------------------------------------------------------

/// `fixtures/corpus/intrinsics.frees`. Trig takes radians — `sin(0) = 0`,
/// `cos(0) = 1`.
#[test]
fn elementary_intrinsics_evaluate_through_the_solver() {
    let solution = solved(
        "a = sqrt(16)\nb = abs(-3.5)\nc = exp(0)\nd = ln(1)\n\
         e = min(3, 7)\nf = max(3, 7)\ng = sin(0)\nh = cos(0)\n",
    );
    assert_near(get(&solution, "a"), 4.0);
    assert_near(get(&solution, "b"), 3.5);
    assert_near(get(&solution, "c"), 1.0);
    assert_near(get(&solution, "d"), 0.0);
    assert_near(get(&solution, "e"), 3.0);
    assert_near(get(&solution, "f"), 7.0);
    assert_near(get(&solution, "g"), 0.0);
    assert_near(get(&solution, "h"), 1.0);
    assert_eq!(solution.blocks.len(), 8);
}

/// `fixtures/corpus/constants.frees`. The expression parser does not fold `#`
/// constants (there is no `ConstantsRegistry` module for it to call), so the
/// engine has to treat them as knowns or the system is underdetermined.
#[test]
fn built_in_constants_do_not_count_as_degrees_of_freedom() {
    let report = check("a = pi#\nb = R#\nc = g#\n").unwrap();
    assert!(report.solvable, "{}", report.message);
    assert_eq!(report.equation_count, 3);
    assert_eq!(report.unknown_count, 3);
    assert_eq!(report.variables, vec!["a", "b", "c"]);

    let solution = solved("a = pi#\nb = R#\nc = g#\n");
    assert_near(get(&solution, "a"), std::f64::consts::PI);
    assert_near(get(&solution, "b"), 8.314_462_618);
    assert_near(get(&solution, "c"), 9.806_65);
    // A folded constant is not a result row: `fixtures/golden/constants.json`
    // lists exactly `a`, `b`, `c`.
    assert_eq!(keys(&solution.values), vec!["a", "b", "c"]);
}

/// `fixtures/corpus/arithmetic.frees`: precedence and associativity, checked
/// through the whole pipeline rather than at the AST.
#[test]
fn precedence_and_associativity_survive_the_pipeline() {
    let solution = solved(
        "a = 2 + 3 * 4\nb = (2 + 3) * 4\nc = 2 ^ 3 ^ 2\n\
         d = -2 ^ 2\ne = 10 / 2 / 5\nf = 2 - 3 - 4\n",
    );
    assert_near(get(&solution, "a"), 14.0);
    assert_near(get(&solution, "b"), 20.0);
    assert_near(get(&solution, "c"), 512.0); // ^ is right-associative
    assert_near(get(&solution, "d"), -4.0); // unary minus binds looser than ^
    assert_near(get(&solution, "e"), 1.0);
    assert_near(get(&solution, "f"), -5.0);
}

/// `fixtures/corpus/comments.frees`: all three comment forms, mid-document.
#[test]
fn every_comment_form_is_ignored_by_the_solve() {
    let solution = solved(
        "{ A brace comment }\n\
         a = 1  { trailing brace comment }\n\
         \"a quote comment\"\n\
         b = 2  // a line comment\n\
         c = a + b\n",
    );
    assert_near(get(&solution, "c"), 3.0);
    assert_eq!(solution.blocks.len(), 3);
}

/// A domain error at the starting point is reported as itself, quoting the
/// equation, instead of degenerating into "did not converge".
#[test]
fn a_domain_error_at_the_start_names_the_equation() {
    let err = failed("y = 1\nx = 1 / (y - 1)\n");
    let text = message(&err);
    assert!(text.contains("division by zero"), "{text}");
    assert!(text.contains("x = 1 / (y - 1)"), "{text}");
    assert!(text.starts_with("Block "), "{text}");
}

/// An equation with no real root fails as a solver error naming the block.
#[test]
fn a_block_with_no_real_root_fails_with_its_equation_quoted() {
    let err = failed("exp(x) = -1\n");
    assert!(matches!(err, FreesError::Solver { .. }), "{err:?}");
    let text = message(&err);
    assert!(text.contains("exp(x) = -1"), "{text}");
    assert!(text.starts_with("Block 1"), "{text}");
}

// ---------------------------------------------------------------------------
// A realistic multi-block document
// ---------------------------------------------------------------------------

/// A small engineering document with unit annotations, a simultaneous core and
/// sequential tails — the shape the browser app will actually run.
#[test]
fn a_realistic_document_solves_end_to_end() {
    let solution = solved(
        "{ Steady flow through a resistance, then a mixing balance. }\n\
         P1   = 300 [kPa]\n\
         P2   = 120 [kPa]\n\
         K    = 2.5e-5\n\
         mdot = K * sqrt(P1 - P2)\n\
         { simultaneous: two streams mixing to a known total }\n\
         m_a + m_b = mdot\n\
         m_a - 3 * m_b = 0\n\
         { sequential tail }\n\
         ratio = m_a / m_b\n",
    );

    assert_near(get(&solution, "p1"), 300_000.0);
    assert_near(get(&solution, "p2"), 120_000.0);
    let mdot = 2.5e-5 * (180_000.0f64).sqrt();
    assert_near(get(&solution, "mdot"), mdot);
    assert_near(get(&solution, "m_a"), 0.75 * mdot);
    assert_near(get(&solution, "m_b"), 0.25 * mdot);
    assert_near(get(&solution, "ratio"), 3.0);

    // P1, P2, K, mdot scalar; (m_a, m_b) simultaneous; ratio scalar.
    assert_eq!(solution.blocks.len(), 6);
    let simultaneous: Vec<&Vec<String>> = solution
        .blocks
        .iter()
        .filter(|b| !b.is_scalar())
        .map(|b| &b.variables)
        .collect();
    assert_eq!(simultaneous.len(), 1);
    assert_eq!(*simultaneous[0], vec!["m_a".to_string(), "m_b".to_string()]);

    // Blocks are in solve order: `ratio` cannot precede `m_a`/`m_b`.
    let names: Vec<&str> = solution
        .blocks
        .iter()
        .flat_map(|b| b.variables.iter().map(String::as_str))
        .collect();
    let pos = |needle: &str| names.iter().position(|n| *n == needle).unwrap();
    assert!(pos("mdot") < pos("m_a"));
    assert!(pos("m_a") < pos("ratio"));
    assert!(pos("p1") < pos("mdot"));

    let report = check(
        "P1 = 300 [kPa]\nP2 = 120 [kPa]\nK = 2.5e-5\nmdot = K * sqrt(P1 - P2)\n\
         m_a + m_b = mdot\nm_a - 3 * m_b = 0\nratio = m_a / m_b\n",
    )
    .unwrap();
    assert!(report.solvable);
    assert_eq!(report.equation_count, 7);
    assert_eq!(report.unknown_count, 7);
}

/// A `FOR` body is flattened into the same equation system — one equation per
/// iteration with the loop variable substituted (the Java
/// `EquationParser.flatten` rule; a body not using the index would correctly
/// be refused as the same equation stated N times).
#[test]
fn for_bodies_are_flattened_into_the_system() {
    let solution = solved("FOR i = 1 TO 3\n  a[i] = 5 * i\nEND\nb = a[3] * 2\n");
    assert_near(get(&solution, "a[1]"), 5.0);
    assert_near(get(&solution, "a[2]"), 10.0);
    assert_near(get(&solution, "a[3]"), 15.0);
    assert_near(get(&solution, "b"), 30.0);
    assert_eq!(solution.blocks.len(), 4);
}

/// Solving is deterministic: identical input, identical output, every time.
/// The wasm/native parity story depends on this.
#[test]
fn solving_the_same_document_twice_gives_identical_results() {
    let source = "x^2 + y^3 = 77\nx/y = 1.23456\nz = x + y\n";
    let first = solved(source);
    let second = solved(source);
    assert_eq!(first.values, second.values);
    assert_eq!(first.blocks, second.blocks);
    assert_eq!(first.iterations, second.iterations);
}

// ---------------------------------------------------------------------------
// A2 — display names (first-seen source spelling)
// ---------------------------------------------------------------------------

/// `fixtures/golden/case_insensitive.json`: the Java engine records the
/// spelling of each variable's *first* appearance and keys it by the lowercase
/// canonical name — `t_out` → `"T_out"`, `tin` → `"Tin"` (not the later `TIN`).
#[test]
fn display_names_keep_the_first_seen_spelling() {
    let solution = solved("Tin = 300\nT_out = TIN * 2\nresult = t_Out + tin\n");
    let names: Vec<(&str, &str)> = solution
        .display_names
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        names,
        vec![("result", "result"), ("t_out", "T_out"), ("tin", "Tin")]
    );
}

/// The map covers exactly the result variables: no function names (`sqrt`),
/// no built-in constants (`pi#`), no unit spellings from `[...]` annotations —
/// the golden fixtures record none of those.
#[test]
fn display_names_cover_exactly_the_result_variables() {
    let solution = solved("A = sqrt(16)\nB = 2 * pi#\nP = 140 [kPa]\n");
    assert_eq!(
        keys(&solution.values),
        solution
            .display_names
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    assert_eq!(solution.display_names["a"], "A");
    assert_eq!(solution.display_names["b"], "B");
    assert_eq!(solution.display_names["p"], "P");
}

/// `check` carries the same map, covering its `variables` list.
#[test]
fn check_reports_display_names_too() {
    let report = check("Tin = 300\nT_out = TIN * 2\n").unwrap();
    assert_eq!(report.variables, vec!["t_out", "tin"]);
    assert_eq!(report.display_names["t_out"], "T_out");
    assert_eq!(report.display_names["tin"], "Tin");
}

/// Sigil suffixes stay part of the spelling (`X#` would be a constant; a `$`
/// name is a string — neither shows up as an unknown, but an `_`-ridden
/// mixed-case name must round-trip untouched).
#[test]
fn display_names_keep_sigils_and_underscores_as_written() {
    let solution = solved("UA_chl_R = 5\n");
    assert_eq!(solution.display_names["ua_chl_r"], "UA_chl_R");
}

// ---------------------------------------------------------------------------
// A3 — error positions on check
// ---------------------------------------------------------------------------

/// A parse failure is *data*, not an `Err` — the Java `CheckController`
/// answers 400 with a full CheckResponse body (`solvable=false`, zero counts,
/// `"Syntax error: …"`, `errorLine`, `errors`), and `api.ts` parses that body
/// like any success. `check` mirrors it.
#[test]
fn check_reports_a_syntax_error_with_its_line_and_column() {
    let report = check("a = 1\nb = = 2\nc = 3\n").unwrap();
    assert!(!report.solvable);
    assert_eq!(report.equation_count, 0);
    assert_eq!(report.unknown_count, 0);
    assert!(report.variables.is_empty());
    assert!(
        report.message.starts_with("Syntax error: "),
        "{}",
        report.message
    );
    assert_eq!(report.error_line, Some(2));
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].line, 2);
    assert!(report.errors[0].column >= 1);
    assert!(!report.errors[0].message.is_empty());
}

/// The same failure through `solve` stays an `Err` whose span points at the
/// same 1-based line — the CLI and the boundary derive `errorLine` from it.
#[test]
fn solve_still_errs_on_a_syntax_error_and_the_span_agrees_with_check() {
    let source = "a = 1\nb = = 2\nc = 3\n";
    let err = failed(source);
    let span = err.span().expect("parse errors carry a span");
    let (line, _) = span.line_col(source);
    assert_eq!(Some(line), check(source).unwrap().error_line);
}

/// Structural failures keep the old path: no error line, real counts.
#[test]
fn a_structural_failure_has_no_error_line() {
    let report = check("m + n = 5\n").unwrap();
    assert!(!report.solvable);
    assert_eq!(report.error_line, None);
    assert!(report.errors.is_empty());
    assert_eq!(report.equation_count, 1);
}

// ---------------------------------------------------------------------------
// A4 — residuals, stats, block equation text
// ---------------------------------------------------------------------------

/// Residuals are `lhs - rhs` at the returned values, one per equation in
/// source order, quoting the user's own text.
#[test]
fn residuals_quote_each_equation_and_vanish_at_the_solution() {
    let solution = solved("x^2 + y^3 = 77\nx/y = 1.23456\n");
    assert_eq!(solution.residuals.len(), 2);
    assert_eq!(solution.residuals[0].equation, "x^2 + y^3 = 77");
    assert_eq!(solution.residuals[1].equation, "x/y = 1.23456");
    for r in &solution.residuals {
        assert!(r.residual.abs() < 1e-7, "{}: {}", r.equation, r.residual);
        assert_eq!(r.block, 0, "one simultaneous block solves both");
    }
}

/// Each residual names the Tarjan block that solved its equation — the source
/// order and the solve order disagree on purpose here.
#[test]
fn residuals_carry_their_block_index() {
    let solution = solved("c = b + 1\nb = a * 3\na = 2\n");
    // Blocks solve a, then b, then c; the source lists c, b, a.
    let blocks: Vec<usize> = solution.residuals.iter().map(|r| r.block).collect();
    assert_eq!(blocks, vec![2, 1, 0]);
}

/// `stats` mirrors the Java `Stats`: total iterations, the worst residual, and
/// an elapsed time the core deliberately cannot measure.
#[test]
fn stats_summarise_the_solve() {
    let solution = solved("x^2 + y^3 = 77\nx/y = 1.23456\nz = x + y\n");
    assert_eq!(solution.stats.iterations, solution.iterations);
    let worst = solution
        .residuals
        .iter()
        .map(|r| r.residual.abs())
        .fold(0.0f64, f64::max);
    assert_eq!(solution.stats.max_residual, worst);
    assert!(solution.stats.max_residual < 1e-7);
    assert_eq!(
        solution.stats.elapsed_ms, None,
        "no clock in core; the boundary fills this"
    );
}

/// `block_equations` aligns with `blocks` and quotes each block's source text.
#[test]
fn block_equations_quote_each_blocks_source_text() {
    let solution = solved("k = 4\np + q = k\np - q = 1\nr = p * q\n");
    assert_eq!(solution.block_equations.len(), solution.blocks.len());
    assert_eq!(solution.block_equations[0], vec!["k = 4"]);
    assert_eq!(solution.block_equations[1], vec!["p + q = k", "p - q = 1"]);
    assert_eq!(solution.block_equations[2], vec!["r = p * q"]);
}

/// The unit checker is wired into both entry points: a literal-declared unit
/// is reported for the assigned variable, and a variable computed from it gets
/// its unit derived dimensionally. Case with units: P is declared by the
/// annotated literal (converted to its SI display name at parse time) and Q
/// inherits the same dimensions through the multiplication.
#[test]
fn units_are_inferred_by_the_wired_checker() {
    let solution = solved("P = 140 [kPa]\nQ = P * 2\n");
    assert_eq!(
        solution.inferred_units.get("p").map(String::as_str),
        Some("Pa")
    );
    assert_eq!(
        solution.inferred_units.get("q").map(String::as_str),
        Some("Pa")
    );
    assert!(
        solution.unit_warnings.is_empty(),
        "{:?}",
        solution.unit_warnings
    );

    let report = check("P = 140 [kPa]\nQ = P * 2\n").unwrap();
    assert_eq!(
        report.inferred_units.get("p").map(String::as_str),
        Some("Pa")
    );
    assert_eq!(
        report.inferred_units.get("q").map(String::as_str),
        Some("Pa")
    );
    assert!(
        report.unit_warnings.is_empty(),
        "{:?}",
        report.unit_warnings
    );
}

/// A dimensionally inconsistent document still solves/checks — unit problems
/// are warnings, never errors (parent-engine invariant).
#[test]
fn unit_warnings_never_block_anything() {
    let solution = solved("x = 2 [m]\ny = 3 [s]\nz = x + y\n");
    assert!(
        solution
            .unit_warnings
            .iter()
            .any(|w| w.contains("[m]") && w.contains("[s]")),
        "{:?}",
        solution.unit_warnings
    );
    assert!((solution.values["z"] - 5.0).abs() < 1e-12);

    let report = check("x = 2 [m]\ny = 3 [s]\nz = x + y\n").unwrap();
    assert!(report.solvable);
    assert!(!report.unit_warnings.is_empty());
}

/// An external `VariableInfo` unit feeds the checker (T's kelvin grounds the
/// derivation for U) and wins over everything in the solve-path map, but the
/// check-path report leaves the externally declared name out — the Java
/// CheckController composes `deriveUnits` + `inferUnits` only.
#[test]
fn override_units_ground_the_checker() {
    let with_unit = VariableOverride {
        name: "T".into(),
        unit: Some("K".into()),
        ..VariableOverride::default()
    };
    let source = "T = 300\nU = T * 2\n";

    let solution = solve_with(
        source,
        &SolverSettings::default(),
        std::slice::from_ref(&with_unit),
    )
    .unwrap();
    assert_eq!(
        solution.inferred_units.get("t").map(String::as_str),
        Some("K")
    );
    assert_eq!(
        solution.inferred_units.get("u").map(String::as_str),
        Some("K")
    );

    let report = check_with(source, &[with_unit]).unwrap();
    assert_eq!(report.inferred_units.get("t"), None);
    assert_eq!(
        report.inferred_units.get("u").map(String::as_str),
        Some("K")
    );
}

// ---------------------------------------------------------------------------
// A5 — external variable information (solve_with / check_with)
// ---------------------------------------------------------------------------

fn guess_override(name: &str, guess: f64) -> VariableOverride {
    VariableOverride {
        name: name.into(),
        guess: Some(guess),
        ..VariableOverride::default()
    }
}

/// An external guess steers the solve exactly as an in-text GUESS would.
#[test]
fn an_override_guess_selects_the_root() {
    let negative = solve_with(
        "x ^ 2 = 9\n",
        &SolverSettings::default(),
        &[guess_override("x", -3.0)],
    )
    .unwrap();
    assert_near(get(&negative, "x"), -3.0);

    // Names are case-insensitive like everything else.
    let positive = solve_with(
        "x ^ 2 = 9\n",
        &SolverSettings::default(),
        &[guess_override("X", 3.0)],
    )
    .unwrap();
    assert_near(get(&positive, "x"), 3.0);
}

/// The merge rule found in `EquationSystemSolver.withTextGuesses`: in-text
/// GUESS directives merge **over** the external specs — text wins, "so a
/// shared document solves identically for its recipient".
#[test]
fn an_in_text_guess_wins_over_an_override() {
    let solution = solve_with(
        "GUESS x = 3\nx ^ 2 = 9\n",
        &SolverSettings::default(),
        &[guess_override("x", -3.0)],
    )
    .unwrap();
    assert_near(get(&solution, "x"), 3.0);
}

/// The parts a directive omits fall back to the override: text bounds pick the
/// branch while the external guess supplies the start — and a stale external
/// guess outside text bounds is clamped onto them (the bounds win).
#[test]
fn text_bounds_merge_with_an_override_guess_and_clamp_it() {
    // `GUESS x [-5, 0]` gives bounds but no guess; the override's +7 start is
    // clamped to the upper bound 0... from which Newton on x^2=9 stalls at the
    // flat spot, so use a start the clamp moves *into* the negative branch.
    let solution = solve_with(
        "GUESS x [-5, -1]\nx ^ 2 = 9\n",
        &SolverSettings::default(),
        &[guess_override("x", 7.0)],
    )
    .unwrap();
    assert_near(get(&solution, "x"), -3.0);
}

/// Override values pass through their declared unit into SI, the
/// `VariableInfoDto.toSpec` conversion: bounds written in Celsius become
/// kelvins before the solver sees them. With bounds enforced, `T = 500` (K)
/// against a 0..100 °C box (273.15..373.15 K) cannot solve — and the partial
/// diagnostics prove the conversion: the failed block's residual is evaluated
/// at the restored initial guess, which is `DEFAULT_GUESS` clamped onto the
/// *converted* lower bound 273.15, not onto 0.
#[test]
fn override_bounds_convert_through_their_unit() {
    let failure = solve_with(
        "T = 500\n",
        &SolverSettings::default(),
        &[VariableOverride {
            name: "T".into(),
            lower: Some(0.0),
            upper: Some(100.0),
            unit: Some("C".into()),
            ..VariableOverride::default()
        }],
    )
    .expect_err("500 K is outside 0..100 C");
    assert_eq!(failure.failed_block_index, Some(0));
    let partial = failure.partial.as_deref().expect("partial diagnostics");
    // 273.15 - 500: the clamp landed on the Celsius-converted bound.
    assert!(
        (partial.residuals[0].residual - (273.15 - 500.0)).abs() < 1e-9,
        "{:?}",
        partial.residuals
    );
}

/// A guess passes through a purely multiplicative unit the same way.
#[test]
fn an_override_guess_converts_through_its_unit() {
    // -0.009 kPa is -9 Pa (SI): the guess lands on the negative branch.
    let solution = solve_with(
        "x ^ 2 = 81\n",
        &SolverSettings::default(),
        &[VariableOverride {
            name: "x".into(),
            guess: Some(-0.009),
            unit: Some("kPa".into()),
            ..VariableOverride::default()
        }],
    )
    .unwrap();
    assert_near(get(&solution, "x"), -9.0);
}

/// The Java `VariableSpec` constructor's three rejections, verbatim.
#[test]
fn invalid_overrides_are_solver_errors() {
    let solve_one = |o: VariableOverride| {
        solve_with("x = 1\n", &SolverSettings::default(), &[o])
            .unwrap_err()
            .error
    };

    let nan = solve_one(guess_override("x", f64::NAN));
    assert!(matches!(nan, FreesError::Solver { .. }), "{nan:?}");
    assert!(message(&nan).contains("contains NaN"), "{}", message(&nan));

    let crossed = solve_one(VariableOverride {
        name: "x".into(),
        lower: Some(10.0),
        upper: Some(0.0),
        ..VariableOverride::default()
    });
    assert!(
        message(&crossed).contains("Lower bound exceeds upper bound"),
        "{}",
        message(&crossed)
    );

    let outside = solve_one(VariableOverride {
        name: "x".into(),
        guess: Some(100.0),
        lower: Some(0.0),
        upper: Some(10.0),
        ..VariableOverride::default()
    });
    assert!(
        message(&outside).contains("outside its bounds"),
        "{}",
        message(&outside)
    );
}

/// An override naming nothing in the system is ignored without a whisper —
/// the Java Variable Information window posts stale rows with every request.
#[test]
fn a_stale_override_is_silently_ignored() {
    let solution = solve_with(
        "x = 1\n",
        &SolverSettings::default(),
        &[guess_override("long_gone", 42.0)],
    )
    .unwrap();
    assert_near(get(&solution, "x"), 1.0);
    assert!(
        solution.diagnostics.is_empty(),
        "{:?}",
        solution.diagnostics
    );
    assert_eq!(keys(&solution.values), vec!["x"]);
}

/// An unknown unit on an override falls back to factor 1 silently — the Java
/// `toSpec` catch-and-default.
#[test]
fn an_unknown_override_unit_falls_back_to_si() {
    let solution = solve_with(
        "x ^ 2 = 9\n",
        &SolverSettings::default(),
        &[VariableOverride {
            name: "x".into(),
            guess: Some(-3.0),
            unit: Some("zorp".into()),
            ..VariableOverride::default()
        }],
    )
    .unwrap();
    assert_near(get(&solution, "x"), -3.0);
}

/// `check_with` validates overrides (same early error surface as solve) but
/// they cannot change the structural verdict.
#[test]
fn check_with_validates_but_ignores_overrides() {
    let report = check_with("x ^ 2 = 9\n", &[guess_override("x", -3.0)]).unwrap();
    assert!(report.solvable);

    let err = check_with("x = 1\n", &[guess_override("x", f64::NAN)]).unwrap_err();
    assert!(message(&err).contains("contains NaN"), "{}", message(&err));
}

/// The plain entry points are the empty-override case, byte for byte.
#[test]
fn solve_and_check_are_thin_wrappers_over_the_with_variants() {
    let source = "x^2 + y^3 = 77\nx/y = 1.23456\n";
    assert_eq!(
        solve(source, &SolverSettings::default()).unwrap(),
        solve_with(source, &SolverSettings::default(), &[]).unwrap()
    );
    assert_eq!(check(source).unwrap(), check_with(source, &[]).unwrap());
}

/// Tightening the settings must not change the answer, only the effort.
#[test]
fn settings_are_honoured() {
    let source = "x ^ 3 - 2 * x - 5 = 0\n";
    let loose = solve(source, &SolverSettings::default()).unwrap();
    let tight = solve(
        source,
        &SolverSettings {
            max_iterations: 500,
            rel_tolerance: 1e-14,
            abs_tolerance: 1e-15,
            ..SolverSettings::default()
        },
    )
    .unwrap();
    assert_near(get(&loose, "x"), get(&tight, "x"));

    // One iteration is not enough to reach the root from the default start.
    let starved = solve(
        source,
        &SolverSettings {
            max_iterations: 1,
            ..SolverSettings::default()
        },
    );
    assert!(starved.is_err(), "{starved:?}");
}
