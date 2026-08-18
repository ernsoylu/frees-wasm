//! The warm-state adapter's acceptance suite
//! (`frees_core::props::rustprop_warm`, reached through
//! [`RustpropBackend::props_si`]).
//!
//! Five things have to hold, and each has a test below:
//!
//! 1. **Warm equals cold.** A warm answer and a cold answer for the same
//!    `(P, Hmass)` / `(P, Smass)` state agree to 1e-9 on `T` and `Dmass`, and
//!    the warm one is the *more* accurate of the two — measured against
//!    rustprop's `(T,P)` flash, which is exact in temperature.
//! 2. **The cold path is the untouched path.** The adapter runs rustprop's own
//!    `HSU_P_flash` and reads the outputs off the state with the same
//!    expressions rustprop's private `keyed_output` uses, so every answer is
//!    bit-for-bit what `rustprop::props_si` would have returned — dome states
//!    included.
//! 3. **Adversarial states fall back.** Near-saturation and in-dome inputs
//!    take the cold path, asserted through the fallback counter rather than
//!    inferred from the answers.
//! 4. **Air is served.** `(P, Hmass)` for the pseudo-pure Air answers today,
//!    through PT solves, where `rustprop::props_si` still declines the pair.
//! 5. **It is fast.** A warm `T(P, Hmass)` costs tens of microseconds, not
//!    hundreds.

#![cfg(feature = "rustprop-backend")]

use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use frees_core::props::propfun::RealFluid;
use frees_core::props::rustprop_backend::RustpropBackend;
use frees_core::props::rustprop_warm;

const B: RustpropBackend = RustpropBackend;

/// The adapter's seed cache and counters are process-global (one property
/// backend per process, exactly like the slot it is installed into), so every
/// test here holds this first.
fn guard() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    rustprop_warm::reset();
    g
}

fn pt(out: &str, t: f64, p: f64, fluid: &str) -> f64 {
    rustprop::props_si(out, "T", t, "P", p, fluid).unwrap_or(f64::NAN)
}

/// `(fluid, T [K], P [Pa])`, single-phase throughout: subcooled liquid,
/// superheated vapour, compressed liquid and supercritical for each fluid.
/// Air's states are the ones a frees document actually asks for.
const GRID: &[(&str, f64, f64)] = &[
    ("Water", 300.0, 1.0e5),
    ("Water", 320.0, 1.0e5),
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
    ("Air", 320.0, 2.0e5),
    ("Air", 500.0, 1.0e6),
];

/// Seeds the adapter for `(fluid, pair)` from a **neighbouring** state — half
/// a percent away in temperature, one percent in pressure, which is inside the
/// locality gate and is the shape of a solver's Newton step.
fn seed_from_neighbour(fluid: &str, t: f64, p: f64, x_key: &str) {
    let (t0, p0) = (t * 0.995, p * 1.01);
    let x0 = pt(x_key, t0, p0, fluid);
    assert!(x0.is_finite(), "{fluid}: neighbour state must exist");
    let served = B.props_si("T", "P", p0, x_key, x0, fluid).unwrap();
    assert!(served.is_finite());
}

/// **Warm equals cold**, and warm is the more accurate of the two in aggregate.
///
/// The comparison needs a word on tolerances. rustprop's cold `(P,X)` flash is
/// upstream's `HSU_P_flash`, which stops on a ~`2^-30` relative bracket in
/// temperature and returns its midpoint — so a cold `T` carries up to ~`1e-9`
/// relative granularity of its own. A density difference is that temperature
/// difference amplified by the state's own thermal expansion,
/// `|dln(rho)/dln(T)|_P = beta*T`, which reaches ~9 at Water 700 K / 30 MPa.
/// The `Dmass` tolerance therefore carries that factor explicitly — anything
/// else would be asserting that cold is more precise than it is. Measured
/// 2026-08-18: worst `e_t` 8.441e-10 (R134a) and worst `e_d`/slack ratio 0.47,
/// so both bands are still live and still earned.
///
/// **Changed 2026-08-18 (Wave-2 integration).** This test used to assert,
/// pointwise, that warm is *at least as close to the exact `(T,P)` state as
/// cold is*. That held only while cold was uniformly coarse. Wave-2 R8
/// replaced rustprop's 30-bit bisection **stand-in** with upstream's own Boost
/// TOMS748, and cold now lands near-exactly at some states — at 2 of these 38
/// it beats warm (e.g. Water Hmass T=600 K, p=1 MPa: cold 3.790e-16 against
/// warm 6.063e-14). That is cold improving, not warm regressing, so the
/// pointwise ordering was the wrong claim to encode. It is replaced by the two
/// claims that are true and are what the adapter actually promises:
///
/// * per state, warm is within `1e-12` of the exact `(T,P)` state
///   (measured worst 6.063e-14);
/// * over the grid, warm's worst error is at least two orders below cold's
///   (measured 6.063e-14 against 8.441e-10 — four orders).
///
/// Note what did NOT change: cold's ~1e-9 granularity is upstream's own
/// convergence tolerance, not the stand-in's. R8 fixed *which root* the solve
/// lands on, not how tightly it is bracketed.
#[test]
fn warm_equals_cold_over_a_grid_across_four_fluids() {
    let _g = guard();
    let mut worst_t = 0.0f64;
    let mut worst_d = 0.0f64;
    let mut worst_truth_warm = 0.0f64;
    let mut worst_truth_cold = 0.0f64;
    let mut checked = 0usize;
    for x_key in ["Hmass", "Smass"] {
        for &(fluid, t, p) in GRID {
            let x = pt(x_key, t, p, fluid);
            let d_exact = pt("Dmass", t, p, fluid);
            assert!(x.is_finite() && d_exact.is_finite(), "{fluid} {t} {p}");

            // Cold: no seed, so the adapter runs the flash. Each of the two
            // outputs gets its own empty cache, or the second would be served
            // by the state the first just cached. Air has no cold (P,X) flash
            // in rustprop at all, so its reference is the exact (T,P) state.
            let (t_cold, d_cold) = if fluid == "Air" {
                (t, d_exact)
            } else {
                let mut cold = Vec::new();
                for out in ["T", "Dmass"] {
                    rustprop_warm::reset();
                    cold.push(B.props_si(out, "P", p, x_key, x, fluid).unwrap());
                    assert_eq!(
                        rustprop_warm::stats().warm,
                        0,
                        "{fluid} {out}: the cold leg must be cold"
                    );
                }
                (cold[0], cold[1])
            };

            // Warm: seeded from a neighbour, then asked for this state.
            rustprop_warm::reset();
            seed_from_neighbour(fluid, t, p, x_key);
            let before = rustprop_warm::stats().warm;
            let t_warm = B.props_si("T", "P", p, x_key, x, fluid).unwrap();
            let d_warm = B.props_si("Dmass", "P", p, x_key, x, fluid).unwrap();
            assert_eq!(
                rustprop_warm::stats().warm - before,
                2,
                "{fluid} {x_key} T={t} p={p:e}: both queries must be served warm"
            );

            let e_t = ((t_warm - t_cold) / t_cold).abs();
            let e_d = ((d_warm - d_cold) / d_cold).abs();
            // beta*T at the exact state: the factor by which a temperature
            // difference shows up in density at constant pressure.
            let beta_t = pt("isobaric_expansion_coefficient", t, p, fluid) * t;
            let d_slack = 1.0e-9 * beta_t.abs().max(1.0);
            assert!(
                e_t <= 1.0e-9,
                "{fluid} {x_key} T={t} p={p:e}: T warm {t_warm} vs cold {t_cold}, rel {e_t:.3e}"
            );
            assert!(
                e_d <= d_slack,
                "{fluid} {x_key} T={t} p={p:e}: Dmass warm {d_warm} vs cold {d_cold}, \
                 rel {e_d:.3e} > {d_slack:.3e} (beta*T = {beta_t:.3})"
            );
            // Warm's accuracy is asserted in ABSOLUTE terms, against the exact
            // (T,P) state — not relative to cold. See the doc comment: the
            // pointwise "warm is at least as accurate as cold" ordering was
            // valid only while cold was uniformly coarse, and Wave-2 R8 ended
            // that. What survives, and is the claim worth making, is that warm
            // is uniformly excellent: worst measured 6.063e-14 over these 38
            // states (Water Hmass T=600 p=1e6) against cold's worst 8.441e-10
            // (R134a) — four orders apart in warm's favour, even though cold
            // now wins at two individual states by landing near-exactly.
            let truth_warm = ((t_warm - t) / t).abs();
            assert!(
                truth_warm <= 1.0e-12,
                "{fluid} {x_key} T={t} p={p:e}: warm is {truth_warm:.3e} off the exact \
                 (T,P) state, past the 1e-12 pin (measured worst 6.063e-14, 2026-08-18)"
            );
            worst_truth_warm = worst_truth_warm.max(truth_warm);
            if fluid != "Air" {
                worst_truth_cold = worst_truth_cold.max(((t_cold - t) / t).abs());
            }
            worst_t = worst_t.max(e_t);
            worst_d = worst_d.max(e_d);
            checked += 1;
        }
    }
    assert_eq!(checked, 2 * GRID.len());
    // The aggregate claim the pointwise ordering used to make: over the whole
    // grid, warm's worst error against the exact state is far below cold's.
    assert!(
        worst_truth_warm * 100.0 <= worst_truth_cold,
        "warm's worst ({worst_truth_warm:.3e}) should stay at least two orders \
         below cold's worst ({worst_truth_cold:.3e})"
    );
    println!(
        "warm vs cold over {checked} states: worst dT/T={worst_t:.3e} dD/D={worst_d:.3e}; \
         vs the exact state, worst warm={worst_truth_warm:.3e} cold={worst_truth_cold:.3e}"
    );
}

/// **The cold path answers with the same doubles** `rustprop::props_si` does.
///
/// The adapter calls `hmolar_p_state` / `p_smolar_state` itself — rather than
/// delegating and then flashing a second time just to fill its cache — and
/// reads the outputs off the returned state. That is only legitimate if the
/// answers are identical, so this asserts **bitwise** equality for every
/// output the adapter serves, over single-phase states and in-dome states
/// alike.
#[test]
fn cold_path_matches_props_si_bitwise() {
    let _g = guard();
    const OUTPUTS: &[&str] = &[
        "T", "Dmass", "Dmolar", "Hmass", "Hmolar", "Smass", "Smolar", "Umass", "Umolar", "Cpmass",
        "Cpmolar", "Cvmass", "Cvmolar",
    ];
    let mut states: Vec<(&str, f64, f64)> = Vec::new();
    for &(fluid, t, p) in GRID {
        if fluid != "Air" {
            states.push((fluid, pt("Hmass", t, p, fluid), p));
        }
    }
    // In-dome states: a quality of 0, 0.5 and 1 on each condensable fluid.
    let mut dome = 0usize;
    for fluid in ["Water", "R134a", "R1234yf"] {
        for (p, q) in [(1.0e5, 0.0), (5.0e5, 0.5), (2.0e6, 1.0)] {
            let h = rustprop::props_si("Hmass", "P", p, "Q", q, fluid).unwrap();
            states.push((fluid, h, p));
            dome += 1;
        }
    }
    assert!(dome == 9 && states.len() > 20);

    for &(fluid, h, p) in &states {
        for out in OUTPUTS {
            rustprop_warm::reset();
            let mine = B.props_si(out, "P", p, "Hmass", h, fluid).unwrap();
            let theirs = rustprop::props_si(out, "P", p, "Hmass", h, fluid).unwrap();
            assert_eq!(
                mine.to_bits(),
                theirs.to_bits(),
                "{out}({fluid}, P={p:e}, Hmass={h}): adapter {mine} vs props_si {theirs}"
            );
        }
    }
}

/// **Everything the adapter does not own** reaches `rustprop::props_si`
/// unchanged — the non-whitelisted outputs, the other input pairs, the
/// incompressibles, and the echo route.
#[test]
fn unowned_calls_are_forwarded_verbatim() {
    let _g = guard();
    let p = 5.0e5;
    let h = pt("Hmass", 400.0, p, "Water");
    for out in [
        "Q",
        "viscosity",
        "conductivity",
        "speed_of_sound",
        "Z",
        "Gmass",
        "Prandtl",
        "isobaric_expansion_coefficient",
        // the echo route: an output that is one of the inputs
        "P",
        "Hmass",
    ] {
        let mine = B.props_si(out, "P", p, "Hmass", h, "Water").unwrap();
        let theirs = rustprop::props_si(out, "P", p, "Hmass", h, "Water").unwrap();
        assert_eq!(mine.to_bits(), theirs.to_bits(), "{out}");
    }
    // Other pairs, and a fluid the HEOS registry does not know.
    assert_eq!(
        B.props_si("Hmass", "T", 300.0, "P", p, "Water").unwrap(),
        pt("Hmass", 300.0, p, "Water")
    );
    let glycol = B
        .props_si("Cpmass", "T", 300.0, "P", p, "INCOMP::MEG[0.50]")
        .unwrap();
    assert!(glycol.is_finite() && glycol > 0.0);
    // None of the above may have touched the warm path in either direction.
    assert_eq!(rustprop_warm::stats(), Default::default());
}

/// **The adversarial set**: states straddling the saturation line, and states
/// inside the dome, must take the cold path even with a perfectly good
/// single-phase seed sitting in the cache one step away.
///
/// Asserted through the counter, not through the answers: an answer that
/// happens to be right for the wrong reason is exactly what this is guarding
/// against.
#[test]
fn near_saturation_and_in_dome_states_take_the_cold_fallback() {
    let _g = guard();
    let mut cases = 0usize;
    for fluid in ["Water", "R134a", "R1234yf"] {
        for p in [1.0e5, 5.0e5, 2.0e6] {
            // The dome edges and the middle, plus enthalpies a whisker inside
            // each edge — the states where a guessed-density solve could pick
            // up a metastable root.
            let hl = rustprop::props_si("Hmass", "P", p, "Q", 0.0, fluid).unwrap();
            let hv = rustprop::props_si("Hmass", "P", p, "Q", 1.0, fluid).unwrap();
            let span = hv - hl;
            for h in [
                hl,
                hl + 1.0e-6 * span,
                hl + 1.0e-3 * span,
                hl + 0.5 * span,
                hv - 1.0e-3 * span,
                hv - 1.0e-6 * span,
                hv,
            ] {
                // Seed the cache from a *good* single-phase state nearby: a
                // subcooled liquid at the same pressure. Without the gates
                // this is precisely the seed that would drag the solve onto a
                // metastable root.
                rustprop_warm::reset();
                let t_sat = rustprop::props_si("T", "P", p, "Q", 0.0, fluid).unwrap();
                let h_seed = pt("Hmass", t_sat * 0.98, p, fluid);
                B.props_si("T", "P", p, "Hmass", h_seed, fluid).unwrap();
                let before = rustprop_warm::stats();
                let answer = B.props_si("T", "P", p, "Hmass", h, fluid).unwrap();
                let after = rustprop_warm::stats();
                assert_eq!(
                    (
                        after.warm - before.warm,
                        after.cold_fallbacks - before.cold_fallbacks
                    ),
                    (0, 1),
                    "{fluid} P={p:e} Hmass={h}: must fall back cold, got {after:?}"
                );
                // And the cold answer is still rustprop's own.
                assert_eq!(
                    answer.to_bits(),
                    rustprop::props_si("T", "P", p, "Hmass", h, fluid)
                        .unwrap()
                        .to_bits()
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 63);
}

/// **Air.** The adapter answers pseudo-pure `(P,Hmass)`/`(P,Smass)` above the
/// critical temperature, because its machinery *is* a sequence of `(T,p)`
/// solves and the root is unique there. The check is a round trip through
/// rustprop's own PT flash.
///
/// **Changed 2026-08-18 (Wave-2 integration).** This test used to open by
/// asserting the premise that *rustprop itself declines `(P,Hmass)` for Air*.
/// Wave-2 R6/R7 ported the pseudo-pure `HSU_P` and `(D,P)` flashes, so that
/// premise is now false: rustprop serves Air on those pairs directly, at every
/// temperature, and agrees with the CoolProp 8.0.0 wheel to ~6e-16 (checked at
/// T = 100 K, p = 1 bar: 100.000_000_000_148_76 against the wheel's
/// 100.000_000_000_148_82).
///
/// Nothing about the adapter changed, and neither did what this test is for.
/// What changed is the consequence of the adapter declining: below `T_crit` it
/// still refuses — its stability gate cannot prove a pseudo-pure root there —
/// but the call now falls through to a rustprop cold path that *answers*
/// instead of one that errors. So the closing assertion checks the adapter's
/// own decision through its counter, plus bitwise identity with rustprop's
/// cold answer, rather than an error that is no longer rustprop's to raise.
#[test]
fn air_p_hmass_and_p_smass_answer_through_pt_solves() {
    let _g = guard();
    assert!(
        rustprop::props_si("T", "P", 1.0e5, "Hmass", 4.0e5, "Air").is_ok(),
        "the premise: Wave-2 R6/R7 landed the pseudo-pure (P,Hmass) flash, so \
         rustprop now serves Air directly"
    );
    for x_key in ["Hmass", "Smass"] {
        for &(fluid, t, p) in GRID.iter().filter(|(f, _, _)| *f == "Air") {
            let x = pt(x_key, t, p, fluid);
            let d = pt("Dmass", t, p, fluid);
            // Cold (unseeded) first, then warm from a neighbour: both must
            // land on the temperature the PT flash was given.
            rustprop_warm::reset();
            let t_cold = B.props_si("T", x_key, x, "P", p, fluid).unwrap();
            let d_cold = B.props_si("Dmass", x_key, x, "P", p, fluid).unwrap();
            rustprop_warm::reset();
            seed_from_neighbour(fluid, t, p, x_key);
            let t_warm = B.props_si("T", "P", p, x_key, x, fluid).unwrap();
            let d_warm = B.props_si("Dmass", "P", p, x_key, x, fluid).unwrap();
            for (label, got) in [("cold", t_cold), ("warm", t_warm)] {
                assert!(
                    ((got - t) / t).abs() <= 1.0e-9,
                    "Air {label} T({x_key}) = {got}, want {t}"
                );
            }
            for (label, got) in [("cold", d_cold), ("warm", d_warm)] {
                assert!(
                    ((got - d) / d).abs() <= 1.0e-9,
                    "Air {label} Dmass({x_key}) = {got}, want {d}"
                );
            }
        }
    }
    // A state below Air's critical temperature has no superancillary to prove
    // a root stable with, so the ADAPTER declines it — it must never guess
    // there. Since Wave-2 R6/R7 that decline is no longer visible as an error:
    // rustprop's own pseudo-pure (P,Hmass) flash serves the state. So assert
    // the adapter's decision where it is actually recorded — the counter — and
    // that the value the caller gets is rustprop's own, bit for bit.
    let h_cold = pt("Hmass", 100.0, 1.0e5, "Air");
    assert!(h_cold.is_finite());
    rustprop_warm::reset();
    let answer = B
        .props_si("T", "P", 1.0e5, "Hmass", h_cold, "Air")
        .expect("rustprop serves sub-critical Air (P,Hmass) since Wave-2 R6/R7");
    let stats = rustprop_warm::stats();
    assert_eq!(
        (stats.warm, stats.cold_fallbacks),
        (0, 1),
        "sub-critical Air must be declined by the adapter, not guessed: {stats:?}"
    );
    assert_eq!(
        answer.to_bits(),
        rustprop::props_si("T", "P", 1.0e5, "Hmass", h_cold, "Air")
            .unwrap()
            .to_bits(),
        "the declined call must return rustprop's own answer, unmodified"
    );
}

/// **Cost.** Reported always, asserted only in a release build: a debug build
/// of the Helmholtz kernels is an order of magnitude slower and the number
/// would mean nothing.
///
/// The median of many calls is what is asserted, because this machine may be
/// running other builds while the suite runs. The cold cost is measured
/// alongside it, from the same loop shape with the cache emptied each time, so
/// the speed-up is a ratio of two numbers taken under the same conditions
/// rather than one number against a remembered one.
#[test]
fn warm_t_of_p_hmass_costs_tens_of_microseconds() {
    let _g = guard();
    // A debug build only reports, so it does not need the long run.
    let n: usize = if cfg!(debug_assertions) { 200 } else { 2000 };
    let median_of = |mut samples: Vec<f64>| -> f64 {
        samples.sort_by(f64::total_cmp);
        samples[samples.len() / 2]
    };
    for (fluid, t, p) in [("Water", 400.0, 5.0e5), ("Air", 300.0, 1.0e5)] {
        let h = pt("Hmass", t, p, fluid);
        // Move the query a little each call so this measures a solve and not a
        // cache read that happens to be exact.
        let query = |i: usize| h * (1.0 + 1.0e-7 * f64::from((i % 21) as u32));

        // Warm the cache and the CPU.
        rustprop_warm::reset();
        for i in 0..200 {
            B.props_si("T", "P", p, "Hmass", query(i), fluid).unwrap();
        }
        let mut warm = Vec::with_capacity(n);
        for i in 0..n {
            let start = Instant::now();
            let got = B.props_si("T", "P", p, "Hmass", query(i), fluid).unwrap();
            warm.push(start.elapsed().as_secs_f64() * 1.0e6);
            assert!(got.is_finite());
        }
        let stats = rustprop_warm::stats();
        assert_eq!(
            stats.cold_fallbacks, 1,
            "{fluid}: only the first call may be cold, got {stats:?}"
        );

        // The same queries with an empty cache every time — the cold flash.
        // Air has none, so there is nothing to compare it against.
        let cold = if fluid == "Air" {
            f64::NAN
        } else {
            let mut cold = Vec::with_capacity(n / 4);
            for i in 0..n / 4 {
                rustprop_warm::reset();
                let start = Instant::now();
                B.props_si("T", "P", p, "Hmass", query(i), fluid).unwrap();
                cold.push(start.elapsed().as_secs_f64() * 1.0e6);
            }
            median_of(cold)
        };

        let median = median_of(warm);
        println!("{fluid}: warm T(P,Hmass) median {median:.1} us, cold median {cold:.1} us");
        if !cfg!(debug_assertions) {
            assert!(
                median <= 50.0,
                "{fluid}: warm T(P,Hmass) median {median:.1} us exceeds the 50 us budget"
            );
            assert!(
                cold.is_nan() || median * 5.0 <= cold,
                "{fluid}: warm ({median:.1} us) must beat cold ({cold:.1} us) by 5x \
                 or the adapter is not worth its gates"
            );
        }
    }
}

/// The cold-fallback counter is the observability surface, so its arithmetic
/// gets a test of its own: eligible calls land in exactly one bucket, and
/// ineligible ones land in neither.
#[test]
fn the_counters_account_for_every_eligible_call() {
    let _g = guard();
    assert_eq!(rustprop_warm::stats(), Default::default());
    let p = 5.0e5;
    let h = pt("Hmass", 400.0, p, "Water");
    // First: cold (nothing cached).
    B.props_si("T", "P", p, "Hmass", h, "Water").unwrap();
    assert_eq!(rustprop_warm::stats().cold_fallbacks, 1);
    assert_eq!(rustprop_warm::stats().warm, 0);
    // Then three warm ones, including the same state twice.
    for x in [h, h, h * 1.0001] {
        B.props_si("Dmass", "P", p, "Hmass", x, "Water").unwrap();
    }
    assert_eq!(
        rustprop_warm::stats(),
        rustprop_warm::WarmStats {
            warm: 3,
            cold_fallbacks: 1
        }
    );
    // A pair the adapter does not own moves neither counter.
    B.props_si("Hmass", "T", 300.0, "P", p, "Water").unwrap();
    assert_eq!(
        rustprop_warm::stats(),
        rustprop_warm::WarmStats {
            warm: 3,
            cold_fallbacks: 1
        }
    );
    // `reset` clears both the counters and the cached state, so the next call
    // is cold again.
    rustprop_warm::reset();
    B.props_si("T", "P", p, "Hmass", h, "Water").unwrap();
    assert_eq!(
        rustprop_warm::stats(),
        rustprop_warm::WarmStats {
            warm: 0,
            cold_fallbacks: 1
        }
    );
}

/// A warm answer must not depend on which fluid or which pair was asked
/// previously: the cache is keyed on both, and a cross-keyed seed would be a
/// silent wrong answer rather than a slow one.
#[test]
fn the_cache_is_keyed_on_fluid_and_pair() {
    let _g = guard();
    let p = 5.0e5;
    let states: Vec<(&str, f64, f64, f64)> = ["Water", "R134a", "R1234yf"]
        .iter()
        .map(|f| {
            let t = 350.0;
            (*f, t, pt("Hmass", t, p, f), pt("Smass", t, p, f))
        })
        .collect();
    // Interleave the fluids and the two pairs so every call follows a
    // different key's state.
    for _ in 0..3 {
        for &(fluid, t, h, s) in &states {
            let th = B.props_si("T", "P", p, "Hmass", h, fluid).unwrap();
            let ts = B.props_si("T", "P", p, "Smass", s, fluid).unwrap();
            for got in [th, ts] {
                assert!(
                    ((got - t) / t).abs() <= 1.0e-9,
                    "{fluid}: interleaved T = {got}, want {t}"
                );
            }
        }
    }
    let stats = rustprop_warm::stats();
    assert_eq!(stats.cold_fallbacks, 6, "one cold call per (fluid, pair)");
    assert_eq!(stats.warm, 12);
}
