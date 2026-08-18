//! Grading the rustprop-backed humid-air path against CoolProp 8.0.0.
//!
//! Decision D8 (`docs/decisions/0008-coolprop-wasm.md`) recorded
//! `fixtures/humidair/reference.json` — 912 near-ambient HVAC points dumped
//! from the vendored `libCoolProp` 8.0.0 — for exactly this purpose, and said
//! what it expected of whatever backend eventually landed: *"it grades a
//! humid-air implementation against CoolProp 8.0.0 in CI, where no
//! `libCoolProp` exists — including the `coolprop.wasm` one, which should
//! reproduce it to round-off."*
//!
//! This is that grade, for the backend D8 actually got (rustprop, the
//! pure-Rust port, not an emscripten blob — see
//! [`frees_core::props::rustprop_backend`]). Six groups, replayed through the
//! same calls `tools/humidair-ref/gen.py` used to record them:
//!
//! | group | call | points |
//! |---|---|---:|
//! | `psat_water` | `PropsSI("P", "T", T, "Q", 0, "Water")` | 14 |
//! | `w_saturated` | `HAPropsSI("W", "T", T, "R", 1, "P", P)` | 42 |
//! | `h_from_tw` | `HAPropsSI("H", "T", T, "W", W, "P", P)` | 294 |
//! | `h_from_tr` | `HAPropsSI("H", "T", T, "R", R, "P", P)` | 294 |
//! | `h_from_tb` | `HAPropsSI("H", "T", T, "B", B, "P", P)` | 163 |
//! | `b_from_th` | `HAPropsSI("B", "T", T, "H", H, "P", P)` | 105 |
//!
//! `psat_water` is a `PropsSI` group, not psychrometrics: D8 measured the
//! IAPWS-95 *ancillary* saturation equation at 7.1e-5 off CoolProp's own
//! saturation solve and called that "a floor on the `(T,R)` path", so the
//! saturation pressure is graded here as the base of the enhancement factor.
//!
//! # Measured, 2026-08-17
//!
//! D8 hoped for round-off, and got it. Over all 912 points the **median
//! relative error is exactly 0** — 593 of them are bitwise identical to the
//! C++ library's output — and the worst is **4.2110e-3**, carried entirely by
//! one point (below). Excluding that point the worst is **1.5998e-13**, and
//! excluding the whole nested-solve group it is **1.989e-15**.
//!
//! That retires the reason D8 existed. No humid-air entry in
//! `fixtures/tolerances.json` is needed, because there is no humid-air error
//! to tolerate: the ideal-gas model D8 rejected was 2.4e-04 on `(T,W)` and
//! 2.7e-03 on `(T,R)`; this path is at 1.6e-16 and 5.7e-16 on the same points,
//! twelve to thirteen orders better, and the 7.1e-5 ancillary-saturation floor
//! D8 named does not apply because `psat_water` reproduces CoolProp's own
//! saturation solve to 1.3e-16.
//!
//! The one point that is not at round-off is a boundary artefact rather than a
//! port defect — see [`FREEZING_WET_BULB`].
//!
//! # What runs this
//!
//! `cargo test -p frees-core --features rustprop-backend`. The whole file is
//! `cfg`-gated on that feature, so the default `cargo test --workspace` CI job
//! compiles it away — the grade is not yet part of the pipeline, because
//! `rustprop-backend` is not yet a CI feature configuration. Wiring the
//! feature into `.github/workflows/ci.yml` belongs to whoever installs the
//! backend in `install_builtin_once`, which F1 deliberately deferred; this
//! test is ready for that job the moment it exists.

#![cfg(feature = "rustprop-backend")]

use frees_core::props::propfun::RealFluid;
use frees_core::props::rustprop_backend::RustpropBackend;
use std::path::{Path, PathBuf};

/// Round-off bound for the five groups whose answer is a direct evaluation or
/// a single well-conditioned solve.
///
/// Measured worst 1.989e-15 on 2026-08-17 (`b_from_th` at T = 303.15 K,
/// P = 101325 Pa — a 5.7e-13 K absolute miss on a wet-bulb temperature); the
/// other four groups are at 6e-16 or below. Pinned 5x above the worst: this is
/// a round-off bound, so an order of magnitude means the arithmetic changed,
/// not that it drifted.
const ROUNDOFF_TOL: f64 = 1.0e-14;

/// Bound for `h_from_tb` — the one group whose answer is a **nested** solve,
/// and therefore two orders noisier than the rest.
///
/// A `(T, B)` input pair makes upstream run a secant on the humidity ratio
/// whose residual is itself a Brent solve for the wet-bulb temperature, so the
/// inner solve's convergence tolerance shows up in the outer answer. Measured
/// worst 1.5998e-13 on 2026-08-17 (T = 273.15 K, B = 268.15 K, P = 105000 Pa),
/// with the rest of the group at 6e-14 and below and a median of 3.6e-15 —
/// pinned 6x above the worst. [`FREEZING_WET_BULB`] is this group's one point
/// that this bound does *not* cover.
const NESTED_SOLVE_TOL: f64 = 1.0e-12;

/// The one point that is not at round-off: `HAPropsSI("H", "T", 278.15, "B",
/// 273.15, "P", 95000)`, measured **4.2110e-3** relative (44.95 J/kg) on
/// 2026-08-17.
///
/// Why it is a boundary artefact and not a defect. A `(T, B)` input pair is a
/// *nested* solve: upstream iterates a secant on the humidity ratio whose
/// residual is itself a Brent solve for the wet-bulb temperature, and that
/// energy balance switches between liquid water and ice at 273.16 K. A
/// requested wet bulb of 273.15 K sits 10 mK inside the ice branch, right
/// against the discontinuity, so the outer secant has almost no gradient to
/// work with.
///
/// The reference does not resolve this point, and says so three ways. Probed
/// directly against the same vendored `libCoolProp` 8.0.0 that
/// `tools/humidair-ref/gen.py` loads:
///
/// 1. **It flat-lines.** At P = 95000 Pa it answers 10673.853 … 10674.107 J/kg
///    for every B in [273.130, 273.151] — 0.25 J/kg across 21 mK, where the
///    slope is 1.73 J/kg per mK. It is returning nearly the same number
///    whatever wet bulb it is asked for.
/// 2. **It gives up nearby.** B >= 273.155 K returns `inf` at P = 95000 Pa, as
///    does B >= 273.14 K at P = 101325 Pa and P = 105000 Pa — which is why
///    `gen.py` recorded this B at only one of the three pressures.
/// 3. **It contradicts its own forward map.** It reports
///    W(T = 278.15, B = 273.15, P = 95000) = 0.002242969, then answers
///    B = 273.12399 K when asked for the wet bulb of that very W: 26 mK below
///    the wet bulb it was handed. 26 mK of wet bulb is the whole 44.95 J/kg.
///
/// rustprop is smooth and monotone here at P = 95000 Pa — 10717.327 /
/// 10719.056 / 10720.785 J/kg at B = 273.149 / 273.150 / 273.151, exactly the
/// 1.73 J/kg per mK slope — so on this point it is the *better* converged of
/// the two. It is no better off at the other two pressures (it flat-lines
/// there instead, near 9948.3 and 9549.7 J/kg), but the reference recorded no
/// point at those, so nothing is graded there.
///
/// The pin is therefore a *regression* pin, not an accuracy claim: it holds
/// the disagreement where it was measured (4.2110e-3, pinned at 5e-3) so that
/// a port change which widens it, or which loses the answer altogether, trips.
const FREEZING_WET_BULB: (&str, f64, f64, f64) = ("h_from_tb", 278.15, 273.15, 95000.0);

/// The pin on [`FREEZING_WET_BULB`]. Measured 4.2110e-3; ~19% headroom.
const BOUNDARY_TOL: f64 = 5.0e-3;

/// Every point in the file is graded — a silently shrinking reference would
/// otherwise make this test pass by asking less.
const EXPECTED_POINTS: usize = 912;

fn reference_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/humidair/reference.json")
}

/// One graded point: what was asked, what CoolProp said, what rustprop said.
struct Graded {
    group: String,
    label: String,
    want: f64,
    got: f64,
    rel: f64,
    /// True for [`FREEZING_WET_BULB`], which grades against [`BOUNDARY_TOL`].
    boundary: bool,
}

/// Replays one reference row through the call that produced it.
fn replay(backend: &RustpropBackend, group: &str, row: &serde_json::Value) -> (f64, f64, String) {
    let at = |k: &str| -> f64 {
        row[k]
            .as_f64()
            .unwrap_or_else(|| panic!("{group}: row {row} has no numeric '{k}'"))
    };
    let (t, p) = (at("T"), row["P"].as_f64().unwrap_or(f64::NAN));
    // `unwrap` on the call: every one of these 912 points answered on
    // 2026-08-17, so a failure here is a regression to report, not a case to
    // skip. The panic message carries the row.
    let unwrap = |r: frees_core::diag::Result<f64>| {
        r.unwrap_or_else(|e| panic!("{group}: {row} did not answer: {e}"))
    };
    match group {
        "psat_water" => (
            at("p_ws"),
            unwrap(backend.props_si("P", "T", t, "Q", 0.0, "Water")),
            format!("Psat(Water) at T={t} K"),
        ),
        "w_saturated" => (
            at("W"),
            unwrap(backend.ha_props_si("W", "T", t, "R", 1.0, "P", p)),
            format!("W at T={t} K, R=1, P={p} Pa"),
        ),
        "h_from_tw" => (
            at("H"),
            unwrap(backend.ha_props_si("H", "T", t, "W", at("W"), "P", p)),
            format!("H at T={t} K, W={}, P={p} Pa", at("W")),
        ),
        "h_from_tr" => (
            at("H"),
            unwrap(backend.ha_props_si("H", "T", t, "R", at("R"), "P", p)),
            format!("H at T={t} K, R={}, P={p} Pa", at("R")),
        ),
        "h_from_tb" => (
            at("H"),
            unwrap(backend.ha_props_si("H", "T", t, "B", at("B"), "P", p)),
            format!("H at T={t} K, B={} K, P={p} Pa", at("B")),
        ),
        "b_from_th" => (
            at("B"),
            unwrap(backend.ha_props_si("B", "T", t, "H", at("H"), "P", p)),
            format!("B at T={t} K, H={} J/kg, P={p} Pa", at("H")),
        ),
        other => panic!("reference.json grew an ungraded group '{other}'"),
    }
}

/// `median` of a slice already sorted ascending; the lower of the two middles
/// on an even count, which is the conservative half for an error metric.
fn median(sorted: &[f64]) -> f64 {
    sorted[(sorted.len() - 1) / 2]
}

/// The pin a point is graded against — three numbers, each with a reason:
/// the freezing wet bulb, the nested-solve group, everything else.
fn tolerance_for(p: &Graded) -> f64 {
    if p.boundary {
        BOUNDARY_TOL
    } else if p.group == "h_from_tb" {
        NESTED_SOLVE_TOL
    } else {
        ROUNDOFF_TOL
    }
}

#[test]
fn humid_air_reproduces_coolprop_8_to_round_off() {
    let text = std::fs::read_to_string(reference_path()).expect("fixtures/humidair/reference.json");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("reference.json is not JSON");
    assert_eq!(
        doc["coolprop_version"].as_str(),
        Some("8.0.0"),
        "the reference was regenerated from a different CoolProp; re-measure before trusting the pins"
    );

    let backend = RustpropBackend;
    let mut graded: Vec<Graded> = Vec::new();
    // serde_json's default map is ordered, so the report is deterministic.
    for (group, rows) in doc["cases"].as_object().expect("cases is not an object") {
        for row in rows.as_array().expect("a case group is not an array") {
            let (want, got, label) = replay(&backend, group, row);
            let boundary = group == FREEZING_WET_BULB.0
                && row["T"].as_f64() == Some(FREEZING_WET_BULB.1)
                && row["B"].as_f64() == Some(FREEZING_WET_BULB.2)
                && row["P"].as_f64() == Some(FREEZING_WET_BULB.3);
            // The reference carries no zeros (the smallest |want| is
            // 2.66e-06 J/kg, the dry-air enthalpy datum), so a plain relative
            // error needs no scale guard — but guard the division anyway
            // rather than report NaN if the file ever changes.
            let rel = if want == 0.0 {
                (got - want).abs()
            } else {
                (got - want).abs() / want.abs()
            };
            graded.push(Graded {
                group: group.clone(),
                label,
                want,
                got,
                rel,
                boundary,
            });
        }
    }

    assert_eq!(
        graded.len(),
        EXPECTED_POINTS,
        "reference.json no longer holds {EXPECTED_POINTS} points"
    );

    // ---- report ----
    let mut all: Vec<f64> = graded.iter().map(|g| g.rel).collect();
    all.sort_by(|a, b| a.partial_cmp(b).expect("a relative error is NaN"));
    let exact = all.iter().filter(|r| **r == 0.0).count();
    let worst = graded
        .iter()
        .max_by(|a, b| a.rel.partial_cmp(&b.rel).expect("a relative error is NaN"))
        .expect("no points graded");

    println!("humid air vs CoolProp 8.0.0 — {} points", graded.len());
    for (group, rows) in doc["cases"].as_object().expect("cases") {
        let mut g: Vec<f64> = graded
            .iter()
            .filter(|p| &p.group == group)
            .map(|p| p.rel)
            .collect();
        g.sort_by(|a, b| a.partial_cmp(b).expect("NaN"));
        let gw = graded
            .iter()
            .filter(|p| &p.group == group)
            .max_by(|a, b| a.rel.partial_cmp(&b.rel).expect("NaN"))
            .expect("empty group");
        println!(
            "  {group:12} n={:4}  median {:9.3e}  worst {:9.3e}  @ {}",
            rows.as_array().expect("array").len(),
            median(&g),
            gw.rel,
            gw.label
        );
    }
    println!(
        "  OVERALL      n={:4}  median {:9.3e}  worst {:9.3e}  ({exact} of {} bitwise identical)",
        graded.len(),
        median(&all),
        worst.rel,
        graded.len()
    );
    println!(
        "  worst point: {} [{}] want {} got {}",
        worst.label, worst.group, worst.want, worst.got
    );
    let bulk_worst = graded
        .iter()
        .filter(|p| !p.boundary)
        .max_by(|a, b| a.rel.partial_cmp(&b.rel).expect("a relative error is NaN"))
        .expect("no points graded");
    println!(
        "  worst excluding the freezing wet bulb: {:9.3e} @ {} [{}]",
        bulk_worst.rel, bulk_worst.label, bulk_worst.group
    );

    // ---- pins ----
    for p in &graded {
        let tol = tolerance_for(p);
        assert!(
            p.rel < tol,
            "{} [{}]: relative error {:.4e} exceeds {tol:.1e} (want {}, got {})",
            p.label,
            p.group,
            p.rel,
            p.want,
            p.got
        );
    }

    // The boundary point must still be *present* and still be the only
    // outlier: if a future rustprop resolves it, this trips and the exception
    // above should be deleted rather than kept as dead weight.
    let boundary = graded
        .iter()
        .find(|p| p.boundary)
        .expect("reference.json lost the T=278.15 K / B=273.15 K / P=95000 Pa point");
    assert!(
        boundary.rel > NESTED_SOLVE_TOL,
        "the freezing wet-bulb point now agrees to {:.4e} — rustprop resolved it. \
         Delete FREEZING_WET_BULB and grade all {EXPECTED_POINTS} points at the group pins.",
        boundary.rel
    );
    // And it must remain the worst point, so the header number above keeps
    // meaning "one known soft spot" rather than hiding a second one.
    assert!(
        std::ptr::eq(worst, boundary),
        "a point other than the freezing wet bulb is now the worst: {} [{}] at {:.4e}",
        worst.label,
        worst.group,
        worst.rel
    );
}
