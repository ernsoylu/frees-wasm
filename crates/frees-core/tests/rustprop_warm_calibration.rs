//! Calibration sweep for the warm-state adapter's locality gate
//! (`frees_core::props::rustprop_warm`).
//!
//! The gate exists because `PtFlash::solver_rho_tp_guessed` has no stability
//! check: a seed far enough from the query can pull the density solve onto the
//! wrong root. This sweep measures *where* that starts, so the gate can be set
//! inside the safe region rather than guessed.
//!
//! # Method
//!
//! The oracle is rustprop's own `(T,P)` flash, which is exact in `T` by
//! construction — no bisection bracket to blur it. For each base state
//! `(T1, P1)` the sweep takes the exact seed `(T1, rho1)`, then walks a
//! perturbation ladder: for each magnitude `m` it forms a *true* neighbouring
//! state `(T2, P2)`, asks `(T,P)` for that state's `Hmass`/`Smass` and
//! `Dmolar`, and hands the warm solver the pair `(P2, X2)` with the stale seed.
//! The deviation reported is warm-vs-truth, and the truth is not an
//! approximation of anything.
//!
//! `calibration_warm_solve` runs the Newton and the *stability* gate but
//! deliberately **not** the locality gate — that is the thing being
//! calibrated.
//!
//! Run the table:
//! `cargo test -p frees-core --features rustprop-backend --test
//! rustprop_warm_calibration -- --ignored --nocapture`

#![cfg(feature = "rustprop-backend")]

use frees_core::props::rustprop_warm::{
    calibration_warm_solve, CALIBRATION_WARM_MAX_STEPS as MAX_STEPS,
};

/// `(fluid, T [K], P [Pa])` — base states spanning liquid, vapour,
/// compressed-liquid and supercritical regimes for the four fluids this build
/// links.
const BASES: &[(&str, f64, f64)] = &[
    ("Water", 300.0, 1.0e5),
    ("Water", 300.0, 1.0e7),
    ("Water", 450.0, 2.0e5),
    ("Water", 500.0, 1.0e7),
    ("Water", 600.0, 1.0e6),
    ("Water", 700.0, 3.0e7),
    ("R134a", 250.0, 5.0e5),
    ("R134a", 300.0, 2.0e5),
    ("R134a", 340.0, 3.0e6),
    ("R134a", 400.0, 5.0e6),
    ("R1234yf", 250.0, 5.0e5),
    ("R1234yf", 300.0, 2.0e5),
    ("R1234yf", 340.0, 3.0e6),
    ("R1234yf", 400.0, 5.0e6),
    ("Air", 250.0, 5.0e5),
    ("Air", 300.0, 1.0e5),
    ("Air", 500.0, 1.0e6),
];

/// Perturbation magnitudes: `|Δln p|` on the pressure axis, `|ΔT|/T` on the
/// caloric one (the gate measures the caloric offset in temperature-equivalent
/// units, so the two ladders are directly comparable to the constants).
const LADDER: &[f64] = &[
    1.0e-4, 1.0e-3, 1.0e-2, 0.02, 0.05, 0.1, 0.2, 0.35, 0.5, 1.0, 2.0,
];

fn props(out: &str, t: f64, p: f64, fluid: &str) -> Option<f64> {
    rustprop::props_si(out, "T", t, "P", p, fluid).ok()
}

#[derive(Default)]
struct Tally {
    accepted: usize,
    newton: usize,
    stability: usize,
    worst_t: f64,
    worst_d: f64,
    worst_at: String,
}

impl Tally {
    fn note(&mut self, dt: f64, dd: f64, what: &str) {
        self.accepted += 1;
        if dt.max(dd) > self.worst_t.max(self.worst_d) {
            self.worst_at = what.to_string();
        }
        self.worst_t = self.worst_t.max(dt);
        self.worst_d = self.worst_d.max(dd);
    }
}

/// One sweep axis. `along_p` perturbs pressure, otherwise temperature; both
/// when `diagonal`.
fn sweep(caloric_is_h: bool, along_p: bool, diagonal: bool) -> Vec<(f64, Tally)> {
    sweep_with(caloric_is_h, along_p, diagonal, MAX_STEPS)
}

fn sweep_with(
    caloric_is_h: bool,
    along_p: bool,
    diagonal: bool,
    max_steps: usize,
) -> Vec<(f64, Tally)> {
    let x_key = if caloric_is_h { "Hmass" } else { "Smass" };
    let mut rows = Vec::new();
    for &m in LADDER {
        let mut tally = Tally::default();
        for &(fluid, t1, p1) in BASES {
            let Some(rho1) = props("Dmolar", t1, p1, fluid) else {
                continue;
            };
            for sign in [1.0f64, -1.0] {
                let (dp, dt) = match (along_p, diagonal) {
                    (_, true) => (sign * m, sign * m),
                    (true, false) => (sign * m, 0.0),
                    (false, false) => (0.0, sign * m),
                };
                let p2 = p1 * dp.exp();
                let t2 = t1 * (1.0 + dt);
                // The truth: a (T,P) flash, exact in T.
                let (Some(x2), Some(rho2)) =
                    (props(x_key, t2, p2, fluid), props("Dmolar", t2, p2, fluid))
                else {
                    continue;
                };
                match calibration_warm_solve(fluid, caloric_is_h, p2, x2, t1, rho1, max_steps) {
                    Err("stability") => tally.stability += 1,
                    Err(_) => tally.newton += 1,
                    Ok((tw, rhow)) => {
                        let edt = ((tw - t2) / t2).abs();
                        let edd = ((rhow - rho2) / rho2).abs();
                        tally.note(edt, edd, &format!("{fluid} T={t1} p={p1:e} sign={sign}"));
                    }
                }
            }
        }
        rows.push((m, tally));
    }
    rows
}

fn report(name: &str, rows: &[(f64, Tally)]) {
    println!("--- {name} ---");
    println!(
        "{:>8}  {:>4} {:>7} {:>5}  {:>10} {:>10}  worst-at",
        "m", "ok", "newton", "stab", "dT/T", "dD/D"
    );
    for (m, t) in rows {
        println!(
            "{m:>8.0e}  {:>4} {:>7} {:>5}  {:>10.2e} {:>10.2e}  {}",
            t.accepted, t.newton, t.stability, t.worst_t, t.worst_d, t.worst_at
        );
    }
}

/// The calibration table, at three step budgets so the choice of
/// `WARM_MAX_STEPS` is visible next to its consequences. `#[ignore]`d because
/// it is a measurement, not a gate; the assertions that keep the constants
/// honest live in `the_gate_band_converges_and_nothing_outside_it_lies` below.
#[test]
#[ignore = "calibration measurement; run with --ignored --nocapture"]
fn calibration_table() {
    for steps in [3, 4, 5] {
        println!("======== max Newton steps = {steps} ========");
        for (caloric_is_h, label) in [(true, "Hmass"), (false, "Smass")] {
            report(
                &format!("{label}: pressure axis"),
                &sweep_with(caloric_is_h, true, false, steps),
            );
            report(
                &format!("{label}: temperature axis"),
                &sweep_with(caloric_is_h, false, false, steps),
            );
            report(
                &format!("{label}: diagonal"),
                &sweep_with(caloric_is_h, true, true, steps),
            );
        }
    }
}

/// The two properties the shipped gate is set on — `GATE_LN_P = 0.10` on the
/// pressure axis, `GATE_DT_REL = 0.01` on the caloric one, with
/// `WARM_MAX_STEPS = 4` — checked on every run rather than trusted to the
/// one-off measurement the constants were chosen from.
///
/// **Inside the band**, both:
///
/// * every swept state converges — zero refusals, which is the hit rate the
///   gate exists to buy;
/// * every accepted solve reproduces the exact `(T,P)` state to 1e-9 (measured
///   worst ~1e-13, four orders inside).
///
/// **Outside the band**, all the way out to a factor-of-`e^2` pressure change
/// and a tripled temperature, the failure mode must stay a *refusal*: whatever
/// the solve accepts is still the exact state. That is what makes the locality
/// gate a hit-rate knob rather than the only thing standing between the adapter
/// and a metastable root — that job belongs to `accept_state`, which stays on
/// here.
///
/// One sweep, both assertions: each rung of the ladder costs a few hundred
/// flashes, so walking it twice would double a slow test for nothing.
#[test]
fn the_gate_band_converges_and_nothing_outside_it_lies() {
    const GATE_LN_P: f64 = 0.10;
    const GATE_DT_REL: f64 = 0.01;
    let mut worst_t = 0.0f64;
    let mut worst_d = 0.0f64;
    let mut inside = 0usize;
    let mut outside_worst = 0.0f64;
    let mut outside_at = String::new();
    for caloric_is_h in [true, false] {
        // The diagonal moves both axes at once, so it is bounded by the
        // tighter of the two constants — exactly as `seed_is_local` bounds it.
        for (axis, limit, rows) in [
            ("pressure", GATE_LN_P, sweep(caloric_is_h, true, false)),
            (
                "temperature",
                GATE_DT_REL,
                sweep(caloric_is_h, false, false),
            ),
            ("diagonal", GATE_DT_REL, sweep(caloric_is_h, true, true)),
        ] {
            for (m, tally) in &rows {
                let worst = tally.worst_t.max(tally.worst_d);
                // Everywhere on the ladder: an accepted solve is the exact
                // state, gate or no gate.
                assert!(
                    worst <= 1.0e-9,
                    "{axis} axis, m={m:e}: an accepted solve must be the exact state, \
                     got {worst:.3e} at {}",
                    tally.worst_at
                );
                if *m <= limit {
                    assert_eq!(
                        (tally.newton, tally.stability),
                        (0, 0),
                        "{axis} axis, m={m:e}: inside the gate band every state must \
                         converge ({} newton / {} stability refusals)",
                        tally.newton,
                        tally.stability
                    );
                    worst_t = worst_t.max(tally.worst_t);
                    worst_d = worst_d.max(tally.worst_d);
                    inside += tally.accepted;
                } else if worst > outside_worst {
                    outside_worst = worst;
                    outside_at = tally.worst_at.clone();
                }
            }
        }
    }
    assert!(
        inside > 200,
        "the sweep must actually accept states inside the gate: {inside}"
    );
    println!("gate band: {inside} accepted, worst dT/T={worst_t:.3e} dD/D={worst_d:.3e}");
    println!("outside the gate (to m=2.0): worst accepted {outside_worst:.3e} at {outside_at}");
}
