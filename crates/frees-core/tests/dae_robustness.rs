//! Wave G6: property-based fuzzing over the **DAE API surface** — the gap
//! `docs/status-phase12.md`'s "did not deliver" item 4 named ("the DAE surface
//! is still un-fuzzed at API level"). The document-level fuzz cannot reach
//! most of this: a `.frees` document only selects the IDA path through
//! `method = ida`, with the assembler choosing every setting, so the raw
//! `IdaDaeSolver` contract — hostile tolerances, misuse orderings, residuals
//! that lie — was exercised only by the hand-picked oracle cases in
//! `dae/solver_tests.rs`.
//!
//! The contract enforced is the same one line as `fuzz_properties.rs`: every
//! public entry point answers `Ok` or `Err` for any input whatsoever — no
//! panic, no abort, no hang (`max_steps` bounds every property that
//! integrates). Where a generated problem has a closed form, the answer is
//! checked too, so this file is not survival-only:
//!
//! * a linear scalar ODE-as-DAE lands on `y0·e^{a·t}` within integrator
//!   tolerance scaled to the trajectory;
//! * a semi-explicit index-1 pair keeps its algebraic constraint along the
//!   whole trajectory, and `calc_consistent_ic` repairs a deliberately
//!   inconsistent algebraic start;
//! * a linear root crossing is located at the right time with the right
//!   direction sign;
//! * above `SPARSE_THRESHOLD`, the KLU-shaped sparse path and the dense path
//!   agree on the same heat chain — the two linear solvers are
//!   interchangeable or one of them is wrong.
//!
//! Ground truth for the *fixed* cases stays `fixtures/dae-oracle.json`
//! (re-verified bit-identical against live SUNDIALS IDA 6.4.1 + KLU on
//! 2026-08-23, this wave — the probe is no longer frozen); this file covers
//! the input space around them. Any minimized counterexample that survives
//! triage belongs in `dae/solver_tests.rs` as a named regression.

// Native-only, exactly like fuzz_properties.rs: proptest's rand stack does
// not build on wasm32, and CI clippy compiles this target for wasm32.
#![cfg(not(target_arch = "wasm32"))]

use proptest::prelude::*;

use frees_core::dae::assembly::{ClosureResidual, ClosureRootFn};
use frees_core::dae::solver::{
    IdaDaeSolver, IDA_ROOT_RETURN, IDA_YA_YDP_INIT, IDA_Y_INIT, SPARSE_THRESHOLD,
};

/// Bound every integration so a wrong step-size collapse reads as an error,
/// never a hang. IDA's own default is 500 per `step` call; this is per-call.
const MAX_STEPS: u64 = 10_000;

/// Any-float strategy: finite values across magnitudes plus the three
/// non-finite hostiles, weighted toward the finite range the solver should
/// actually survive *and answer* in.
fn any_f64() -> impl Strategy<Value = f64> {
    prop_oneof![
        8 => -1.0e6..1.0e6f64,
        1 => prop_oneof![Just(f64::NAN), Just(f64::INFINITY), Just(f64::NEG_INFINITY)],
        1 => prop_oneof![Just(0.0), Just(-0.0), Just(f64::MIN_POSITIVE), Just(1.0e300)],
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// The whole driver sequence survives arbitrary (including non-finite)
    /// scalar-problem inputs: `new → set_tolerances → set_variable_id → init
    /// → calc_consistent_ic → step`. Every call may refuse; none may panic.
    #[test]
    fn scalar_driver_sequence_survives_any_inputs(
        a in any_f64(),
        b in any_f64(),
        y0 in any_f64(),
        rtol in any_f64(),
        atol in any_f64(),
        tout in any_f64(),
        icopt in prop_oneof![Just(IDA_YA_YDP_INIT), Just(IDA_Y_INIT), Just(0), Just(99)],
    ) {
        let outcome = std::panic::catch_unwind(|| {
            let res = ClosureResidual::new(move |_t, y, yp, r: &mut [f64]| {
                r[0] = yp[0] - (a * y[0] + b);
                Ok(())
            });
            let mut s = IdaDaeSolver::new(1, &res)?;
            s.set_tolerances(rtol, atol);
            s.set_max_steps(MAX_STEPS);
            s.set_variable_id(&[1.0])?;
            s.init(0.0, &[y0], &[a * y0 + b])?;
            s.calc_consistent_ic(icopt, 1.0e-3)?;
            s.step(tout).map(|_| ())
        });
        prop_assert!(
            outcome.is_ok(),
            "IdaDaeSolver panicked on a={a:?} b={b:?} y0={y0:?} rtol={rtol:?} \
             atol={atol:?} tout={tout:?} icopt={icopt}"
        );
    }

    /// On sane inputs the scalar linear DAE is not just survived but SOLVED:
    /// `y' = a·y`, closed form `y0·e^{a·t}`, graded at a tolerance scaled to
    /// what the integrator was asked for.
    #[test]
    fn scalar_linear_dae_matches_the_closed_form(
        a in -3.0..0.5f64,
        y0 in prop_oneof![-100.0..-0.1f64, 0.1..100.0f64],
        tend in 0.5..4.0f64,
    ) {
        let res = ClosureResidual::new(move |_t, y, yp, r: &mut [f64]| {
            r[0] = yp[0] - a * y[0];
            Ok(())
        });
        let mut s = IdaDaeSolver::new(1, &res).unwrap();
        s.set_tolerances(1e-8, 1e-10);
        s.set_max_steps(MAX_STEPS);
        s.set_variable_id(&[1.0]).unwrap();
        s.init(0.0, &[y0], &[a * y0]).unwrap();
        s.calc_consistent_ic(IDA_YA_YDP_INIT, 1.0e-3).unwrap();
        let out = s.step(tend).unwrap();
        let exact = y0 * (a * tend).exp();
        let tol = 1e-5 * y0.abs().max(exact.abs()) + 1e-8;
        prop_assert!(
            (out.y[0] - exact).abs() <= tol,
            "y({tend}) = {} but exact is {exact} (a={a}, y0={y0})",
            out.y[0]
        );
    }

    /// Semi-explicit index-1: `y1' = -k·y1, 0 = y2 − c·y1`, started with a
    /// DELIBERATELY inconsistent algebraic value. `calc_consistent_ic` with
    /// `IDA_YA_YDP_INIT` must repair y2 to c·y1, and the constraint must hold
    /// along the whole trajectory.
    #[test]
    fn consistent_ic_repairs_the_algebraic_start_and_the_constraint_holds(
        k in 0.05..5.0f64,
        c in prop_oneof![-10.0..-0.1f64, 0.1..10.0f64],
        y1_0 in 0.5..50.0f64,
        y2_bogus in -100.0..100.0f64,
    ) {
        let res = ClosureResidual::new(move |_t, y, yp, r: &mut [f64]| {
            r[0] = yp[0] + k * y[0];
            r[1] = y[1] - c * y[0];
            Ok(())
        });
        let mut s = IdaDaeSolver::new(2, &res).unwrap();
        s.set_tolerances(1e-8, 1e-10);
        s.set_max_steps(MAX_STEPS);
        s.set_variable_id(&[1.0, 0.0]).unwrap();
        s.init(0.0, &[y1_0, y2_bogus], &[-k * y1_0, 0.0]).unwrap();
        s.calc_consistent_ic(IDA_YA_YDP_INIT, 1.0e-3).unwrap();
        for i in 1..=4 {
            let t = 0.25 * i as f64;
            let out = s.step(t).unwrap();
            let residual = out.y[1] - c * out.y[0];
            let tol = 1e-6 * (out.y[1].abs().max((c * out.y[0]).abs()) + 1.0);
            prop_assert!(
                residual.abs() <= tol,
                "constraint broke at t={t}: y2 − c·y1 = {residual:e} \
                 (k={k}, c={c}, y1_0={y1_0}, bogus y2(0)={y2_bogus})"
            );
        }
    }

    /// A residual that starts lying mid-integration (NaN after a time
    /// threshold) must produce an `Err` or a truncated `Ok` — never a panic,
    /// never an unbounded retry loop.
    #[test]
    fn a_residual_that_goes_nan_midway_is_survived(
        t_poison in 0.1..0.9f64,
        poison in prop_oneof![Just(f64::NAN), Just(f64::INFINITY)],
    ) {
        let outcome = std::panic::catch_unwind(|| {
            let res = ClosureResidual::new(move |t, y, yp, r: &mut [f64]| {
                r[0] = if t > t_poison { poison } else { yp[0] + y[0] };
                Ok(())
            });
            let mut s = IdaDaeSolver::new(1, &res)?;
            s.set_tolerances(1e-8, 1e-10);
            s.set_max_steps(MAX_STEPS);
            s.set_variable_id(&[1.0])?;
            s.init(0.0, &[1.0], &[-1.0])?;
            s.calc_consistent_ic(IDA_YA_YDP_INIT, 1.0e-3)?;
            s.step(2.0).map(|_| ())
        });
        prop_assert!(
            outcome.is_ok(),
            "poisoned residual panicked (t_poison={t_poison}, poison={poison:?})"
        );
    }

    /// Root finding: `g = t − t_root` crossing inside the span is located at
    /// `t_root` with the increasing direction (+1); a crossing outside the
    /// span never fires.
    #[test]
    fn a_linear_root_crossing_is_located_or_correctly_absent(
        t_root in 0.1..2.9f64,
        tend in 1.0..2.0f64,
    ) {
        let res = ClosureResidual::new(|_t, y, yp, r: &mut [f64]| {
            r[0] = yp[0] + y[0];
            Ok(())
        });
        let root = ClosureRootFn::new(move |t, _y, _yp, g: &mut [f64]| {
            g[0] = t - t_root;
            Ok(())
        });
        let mut s = IdaDaeSolver::new(1, &res).unwrap();
        s.set_tolerances(1e-8, 1e-10);
        s.set_max_steps(MAX_STEPS);
        s.set_variable_id(&[1.0]).unwrap();
        s.set_roots(1, &root);
        s.init(0.0, &[1.0], &[-1.0]).unwrap();
        s.calc_consistent_ic(IDA_YA_YDP_INIT, 1.0e-3).unwrap();
        let out = s.step(tend).unwrap();
        if t_root < tend {
            prop_assert_eq!(out.flag, IDA_ROOT_RETURN, "crossing inside the span must fire");
            prop_assert!(
                (out.t - t_root).abs() <= 1e-6 * (1.0 + t_root),
                "root located at {} but planted at {t_root}",
                out.t
            );
            prop_assert_eq!(out.roots_found[0], 1, "t − t_root increases through zero");
        } else {
            prop_assert!(
                out.flag != IDA_ROOT_RETURN && (out.t - tend).abs() <= 1e-9 * tend,
                "no crossing inside the span, yet flag={} t={}",
                out.flag,
                out.t
            );
        }
    }

    /// Above `SPARSE_THRESHOLD` the sparse (KLU-shaped) and dense linear
    /// paths must agree: the same conduction chain, one solver with the
    /// assembler's tridiagonal pattern and one without, graded cell by cell.
    #[test]
    fn sparse_and_dense_paths_agree_on_a_heat_chain(
        kappa in 0.2..3.0f64,
        hot in 350.0..500.0f64,
    ) {
        let n = SPARSE_THRESHOLD + 3;
        let run = |sparse: bool| -> Vec<f64> {
            let res = ClosureResidual::new(move |_t, y: &[f64], yp: &[f64], r: &mut [f64]| {
                let nn = y.len();
                for i in 0..nn {
                    let left = if i == 0 { hot } else { y[i - 1] };
                    let right = if i == nn - 1 { 300.0 } else { y[i + 1] };
                    r[i] = yp[i] - kappa * (left - 2.0 * y[i] + right);
                }
                Ok(())
            });
            let mut s = IdaDaeSolver::new(n, &res).unwrap();
            s.set_tolerances(1e-8, 1e-10);
            s.set_max_steps(MAX_STEPS);
            s.set_variable_id(&vec![1.0; n]).unwrap();
            if sparse {
                let pattern: Vec<Vec<usize>> = (0..n)
                    .map(|i| {
                        let mut cols = vec![i];
                        if i > 0 {
                            cols.push(i - 1);
                        }
                        if i + 1 < n {
                            cols.push(i + 1);
                        }
                        cols.sort_unstable();
                        cols
                    })
                    .collect();
                s.set_sparsity(&pattern).unwrap();
            }
            let y0 = vec![300.0; n];
            let yp0: Vec<f64> = (0..n)
                .map(|i| {
                    let left = if i == 0 { hot } else { 300.0 };
                    kappa * (left - 2.0 * 300.0 + 300.0)
                })
                .collect();
            s.init(0.0, &y0, &yp0).unwrap();
            s.calc_consistent_ic(IDA_YA_YDP_INIT, 1.0e-3).unwrap();
            s.step(0.5).unwrap().y
        };
        let dense = run(false);
        let sparse = run(true);
        for (i, (d, sp)) in dense.iter().zip(&sparse).enumerate() {
            let tol = 1e-5 * d.abs().max(1.0);
            prop_assert!(
                (d - sp).abs() <= tol,
                "cell {i}: dense {d} vs sparse {sp} (kappa={kappa}, hot={hot})"
            );
        }
    }
}

/// API misuse answers typed errors, never panics — the hand-written corner
/// set proptest shrinking would otherwise rediscover slowly.
#[test]
fn api_misuse_is_refused_not_panicked() {
    let res = ClosureResidual::new(|_t, y, yp, r: &mut [f64]| {
        r[0] = yp[0] + y[0];
        Ok(())
    });

    assert!(IdaDaeSolver::new(0, &res).is_err(), "n = 0 must be refused");

    let mut s = IdaDaeSolver::new(1, &res).unwrap();
    assert!(s.step(1.0).is_err(), "step before init must be refused");
    assert!(
        s.calc_consistent_ic(IDA_YA_YDP_INIT, 1.0).is_err(),
        "calc_consistent_ic before init must be refused"
    );
    assert!(
        s.set_variable_id(&[1.0, 0.0]).is_err(),
        "wrong-length variable id must be refused"
    );
    assert!(
        s.init(0.0, &[1.0, 2.0], &[0.0]).is_err(),
        "wrong-length state must be refused"
    );
    assert!(
        s.set_sparsity(&[vec![0], vec![1]]).is_err(),
        "wrong-row-count sparsity must be refused"
    );

    s.set_variable_id(&[1.0]).unwrap();
    s.init(0.0, &[1.0], &[-1.0]).unwrap();
    assert!(
        s.calc_consistent_ic(42, 1.0).is_err(),
        "an unknown icopt must be refused"
    );
}
