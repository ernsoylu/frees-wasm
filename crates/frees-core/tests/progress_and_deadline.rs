//! The two host-installed channels a long browser solve needs: the wall-clock
//! deadline's *reporting*, and the progress bar.
//!
//! Both are installed only by the wasm boundary, so nothing here is exercised
//! by the parity replay — which is exactly why they need tests of their own.
//!
//! The deadline half is a regression test with a real history. Up to Wave T5 a
//! struck transient budget was masked: the block that owns a `FinalValue(...)`
//! wraps the transient, so the failed residual sent
//! `solve_block_with_fallback` down the retry ladder, and the merge rescue's
//! `?` replaced the honest "exceeded its N-second wall-clock budget" with
//! whatever the re-run happened to fail on. In the browser that surfaced as
//! "Newton iteration stalled … unable to bracket the (p,X) solution", which
//! sends a user hunting for guess values that were never the problem.

use frees_core::solver::SolverSettings;

/// A transient whose result is read back through `FinalValue`, which is what
/// puts the integration *inside* a Newton block — the shape the masking needed.
/// Deliberately tiny: every assertion here is about plumbing, not physics.
const TRANSIENT: &str = "\
y_final = FinalValue('y')\n\
DYNAMIC relax(method = ode45, time = 0 .. 10, points = 20)\n\
  der(y) = -y / 2\n\
  y(0) = 1\n\
END\n\
";

/// Clears both channels however a test ends, so one failure cannot leak a
/// thread-local into the next test on the same runner thread.
struct Cleanup;

impl Drop for Cleanup {
    fn drop(&mut self) {
        frees_core::ode::deadline::clear();
        frees_core::progress::clear();
    }
}

#[test]
fn a_struck_transient_budget_reports_itself_and_not_a_downstream_newton_failure() {
    let _cleanup = Cleanup;
    // Struck from the very first check, which is the worst case for the
    // masking: every ladder rung re-enters the transient and fails afresh.
    frees_core::ode::deadline::install(
        Box::new(|| true),
        "DYNAMIC: the transient exceeded its 60-second wall-clock budget and \
         was stopped."
            .to_string(),
    );

    let failure = frees_core::engine::solve(TRANSIENT, &SolverSettings::default())
        .expect_err("a struck deadline must fail the solve");
    let message = failure.to_string_message();

    assert!(
        message.contains("wall-clock budget"),
        "the budget must report itself; got: {message}"
    );
    // The specific lie this replaced. A stalled Newton is what the ladder's
    // re-runs produce once the transient can no longer run at all.
    assert!(
        !message.contains("Newton iteration stalled"),
        "the ladder ran on a struck deadline; got: {message}"
    );
}

#[test]
fn no_deadline_installed_leaves_the_same_document_solving() {
    let _cleanup = Cleanup;
    frees_core::ode::deadline::clear();
    let solution = frees_core::engine::solve(TRANSIENT, &SolverSettings::default())
        .expect("the document solves with no budget installed");
    // e^-5 ≈ 6.74e-3 — checked loosely, since the point is that it ran at all.
    let y = solution
        .values
        .get("y_final")
        .copied()
        .expect("y_final is solved");
    assert!(
        (y - (-5.0f64).exp()).abs() < 1e-4,
        "y_final = {y}, want ≈ {}",
        (-5.0f64).exp()
    );
}

#[test]
fn a_solve_reports_progress_that_only_ever_moves_forward() {
    let _cleanup = Cleanup;
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::<f64>::new()));
    let sink = std::rc::Rc::clone(&seen);
    frees_core::progress::install(Box::new(move |f| sink.borrow_mut().push(f)));

    frees_core::engine::solve(TRANSIENT, &SolverSettings::default()).expect("solves");
    frees_core::progress::clear();

    let seen = seen.borrow();
    assert!(
        seen.len() > 2,
        "a transient should report more than its block boundaries; got {seen:?}"
    );
    assert!(
        seen.iter().all(|f| (0.0..=1.0).contains(f)),
        "every report must be a fraction; got {seen:?}"
    );
    // Monotone is the property a bar needs: a block's span is a slice of the
    // bar and the integrator only ever advances within it, so nothing may go
    // backwards even though two different sites report.
    assert!(
        seen.windows(2).all(|w| w[1] >= w[0]),
        "progress went backwards: {seen:?}"
    );
}

/// The IDA path is a separate stepper with a separate reporting site, so it
/// needs its own coverage — the explicit-method test above never enters it.
#[test]
fn the_ida_path_reports_progress_too() {
    let _cleanup = Cleanup;
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::<f64>::new()));
    let sink = std::rc::Rc::clone(&seen);
    frees_core::progress::install(Box::new(move |f| sink.borrow_mut().push(f)));

    let source = "\
y_final = FinalValue('y')\n\
DYNAMIC relax(method = ida, time = 0 .. 10, points = 20)\n\
  der(y) = -y / 2\n\
  y(0) = 1\n\
END\n\
";
    frees_core::engine::solve(source, &SolverSettings::default()).expect("solves");
    frees_core::progress::clear();

    let seen = seen.borrow();
    assert!(
        seen.iter().any(|&f| f > 0.5),
        "the IDA loop never reported past halfway: {seen:?}"
    );
    assert!(
        seen.windows(2).all(|w| w[1] >= w[0]),
        "progress went backwards: {seen:?}"
    );
}

#[test]
fn progress_is_silent_when_no_sink_is_installed() {
    let _cleanup = Cleanup;
    frees_core::progress::clear();
    assert!(!frees_core::progress::active());
    // The native and parity paths run exactly this way; the assertion is that
    // solving does not install one behind their back.
    frees_core::engine::solve(TRANSIENT, &SolverSettings::default()).expect("solves");
    assert!(!frees_core::progress::active());
}

#[test]
fn the_sink_is_dropped_by_clear_so_the_next_solve_is_quiet() {
    let _cleanup = Cleanup;
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::<f64>::new()));
    let sink = std::rc::Rc::clone(&seen);
    frees_core::progress::install(Box::new(move |f| sink.borrow_mut().push(f)));
    frees_core::engine::solve("a = 1\nb = a + 1\n", &SolverSettings::default()).expect("solves");
    let first = seen.borrow().len();
    assert!(first > 0, "the sink saw nothing at all");

    frees_core::progress::clear();
    frees_core::engine::solve("c = 2\nd = c + 1\n", &SolverSettings::default()).expect("solves");
    assert_eq!(
        seen.borrow().len(),
        first,
        "a cleared sink was still being called"
    );
}
