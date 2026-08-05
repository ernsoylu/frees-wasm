//! Adversarial robustness for the measurement surface: resampling,
//! decimation, raster construction and calculated signals. (MDF4 reading was
//! removed in decision D6; its attack tests went with it.)
//!
//! Same rule as [`tests/dynamics_robustness.rs`](dynamics_robustness.rs):
//! **every entry point answers with a `Result` in bounded time.** Not a panic,
//! not an abort, not a hang, and not a plausible-looking wrong answer. The
//! stakes are higher here than anywhere else in the port, for two reasons the
//! module docs state and this file tests:
//!
//! * the inputs are **columns from arbitrary CSV files and formulas typed by
//!   the user** — hostile values (NaN, infinities, denormals) are the normal
//!   case, not the exotic one;
//! * the wasm release profile is `panic = "abort"`, so a panic is not a
//!   diagnostic the shim can render. It is the tab.
//!
//! Several of these tests are regressions for defects the robustness audit
//! found and fixed, sharing a shape: a bound that looked sufficient because
//! it was stated in the wrong unit. (The MDF4 attack surface — corrupt block
//! graphs, forged links, implausible cycle counts — was removed with the
//! format reader itself in decision D6; its regression tests went with it.)
//!
//! * [`a_span_between_two_infinities_is_refused_rather_than_answered_empty`] —
//!   `inf - inf` is `NaN`, which slips past `t1 >= t0`, and `(NaN + 1.0) as u64`
//!   saturates to zero. `fixed` answered an empty raster, so the whole
//!   calculated signal came back as a successful, empty column.
//!   (`raster.rs::fixed`.)
//! * [`the_product_of_the_input_count_and_the_raster_length_is_bounded`] —
//!   `calc::evaluate` holds one raster-length column per bound input at once,
//!   so its working set is `raster × inputs`, and *both* factors were capped
//!   while their product was not. The wasm boundary's `MAX_INPUTS` counts
//!   inputs (128) and its `MAX_INPUT_SAMPLES` counts source samples, so 128
//!   one-point inline series satisfies both and still asks for 128 full-length
//!   columns. Measured with a counting global allocator through
//!   `measurement_calc`: a **5 604-byte** request body peaked at **1 044 MB**,
//!   a 186 000× amplification, at a `collect()` — an abort under
//!   `panic = "abort"`, not a diagnostic. (`calc.rs::MAX_INPUT_COLUMN_SAMPLES`.)
//! * [`a_formula_full_of_time_operators_cannot_allocate_a_column_per_term`] —
//!   the same product reached from the formula instead of the input list: each
//!   `delta`/`integral`/`movavg`/`delay` *occurrence* becomes its own
//!   full-length synthetic column. 200 `delta(x)` terms over a million points
//!   peaked at **1 616 MB**, from one input and 1.6 kB of formula text.
//!   (`calc.rs::MAX_SYNTHETIC_SAMPLES`.)
//!
//! Two more are regressions from a separate **numeric-parity** sweep, which ran
//! every one of these functions against a live JDK 26 build of the reference
//! and compared the answers as raw bits. Those two share the opposite shape:
//! arithmetic that is right at the scale anyone tests it and wrong at the edges.
//!
//! * [`suggest_dt_does_not_skip_a_rung_below_the_literal_table`] —
//!   `raster.rs::pow10` deferred every decade outside `[-30, 30]` to
//!   `libm::pow`, whose fdlibm lineage is not correctly rounded there. Java's
//!   `Math.pow` is. The damage is a skipped rung of the 1-2-5 ladder, not an
//!   ULP. (`raster.rs::pow10`.)
//! * [`a_point_count_past_two_to_the_53_is_not_swallowed_by_its_own_plus_one`] —
//!   `fixed` added its `+ 1` in `f64`, where it is a no-op past 2⁵³, so the
//!   point count quoted in the refusal was one short of the Java's.
//!   (`raster.rs::fixed`.)
//!
//! Two more come from a **malformed-input** sweep, and they are the two places
//! where a bound existed and simply did not cover the axis the attack used:
//!
//! * [`a_wide_formula_is_refused_instead_of_wedging_the_worker`] — `calc.rs`
//!   bounded a formula's *depth* and nothing bounded its *node count*, so a
//!   shallow enormous formula was unbounded in three directions at once: 51 s to
//!   evaluate a 24 kB one over a million-point raster (fourteen minutes for a
//!   megabyte), 6.2 s to *compile* a 90 kB one over a **four-point** raster, and
//!   781 MB of synthetic columns from 14 kB. Only the third is bytes, so only
//!   the third was catchable by any of the sample budgets above.
//!   (`calc.rs::MAX_FORMULA_NODES`, and the per-`Call` binding table `evaluate`
//!   now builds once.)
//! * [`a_moving_average_recovers_once_a_non_finite_sample_leaves_the_window`] —
//!   the only *silent wrong answer* this sweep found. `movavg`'s running sum is
//!   a one-way door: one `±∞` sample, or two large finite ones, poisoned it for
//!   the rest of the channel and every later point came back `NaN` — a gap, over
//!   data that was fine. (`calc.rs::movavg`.)
//! * [`a_moving_average_recovers_over_a_window_of_realistic_width`] — the
//!   *first* repair for the line above only worked on a window of a couple of
//!   samples. It recomputed on every non-finite accumulator, including the
//!   points where the bad sample is still in the window and the recompute cannot
//!   succeed, so its budget was gone before the repair that mattered: a 2 s
//!   window at 1 kHz still left 195 998 fabricated gaps. Found by verifying that
//!   fix instead of trusting it. (`calc.rs::movavg`, gated on the window's ±∞
//!   population.)
//!
//! The rest is the standing corpus: truncated and lying headers, `NaN` and
//! infinite times, ragged channels, descending and stalled time masters,
//! degenerate windows, and formulas at the parser's depth ceiling.

use std::collections::BTreeMap;

use frees_core::ast::Expr;
use frees_core::measurement::calc::{contains_call, evaluate, parse_formula};
use frees_core::measurement::decimate::{lower_bound, min_max};
use frees_core::measurement::raster::{fixed, suggest_dt, union};
use frees_core::measurement::series::{Interp, SampledSeries};
use frees_core::measurement::MeasurementError;

// ── helpers ─────────────────────────────────────────────────────────────────

/// A deterministic LCG, so a failure reproduces without a seed file.
struct Lcg(u64);

impl Lcg {
    fn new() -> Lcg {
        Lcg(0x2545_F491_4F6C_DD1D)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() >> 33) as usize % n
    }

    /// A value from the hostile pool: ordinary numbers, both zeroes, both
    /// infinities, `NaN`, and the extremes of the format.
    fn hostile(&mut self) -> f64 {
        const POOL: [f64; 14] = [
            0.0,
            -0.0,
            1.0,
            -1.0,
            0.5,
            1e-300,
            1e300,
            f64::MAX,
            f64::MIN,
            f64::MIN_POSITIVE,
            f64::EPSILON,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
        ];
        POOL[self.below(POOL.len())]
    }
}

fn series(t: &[f64], v: &[f64], interp: Interp) -> SampledSeries {
    SampledSeries::new(t.to_vec(), v.to_vec(), interp)
}

fn bind(pairs: &[(&str, SampledSeries)]) -> BTreeMap<String, SampledSeries> {
    pairs
        .iter()
        .map(|(name, s)| ((*name).to_string(), s.clone()))
        .collect()
}

fn ramp(n: usize, dt: f64) -> Vec<f64> {
    (0..n).map(|i| i as f64 * dt).collect()
}

/// Evaluate `formula` and require it to answer *something* — a value column or
/// a typed error, never a panic.
fn answered(formula: &str, raster: &[f64], inputs: &BTreeMap<String, SampledSeries>) -> String {
    let parsed = match parse_formula(formula) {
        Ok(e) => e,
        Err(e) => return format!("parse: {e}"),
    };
    match evaluate(&parsed, raster, inputs) {
        Ok(v) => format!("ok: {} point(s)", v.len()),
        Err(e) => format!("eval: {e}"),
    }
}

// ────────────────────────────────────────────────────────────────────────────
// SampledSeries — hostile time masters
// ────────────────────────────────────────────────────────────────────────────

/// `at` binary-searches `t`, and nothing in the format requires a recorded time
/// master to ascend. Every combination of a corrupt master and a corrupt query
/// has to answer, and under [`Interp::Step`] — which never does arithmetic —
/// the answer must be a sample the file actually contains.
#[test]
fn a_corrupt_time_master_never_panics_and_step_never_invents_a_value() {
    let mut rng = Lcg::new();
    for _ in 0..20_000 {
        let n = rng.below(9);
        let t: Vec<f64> = (0..n).map(|_| rng.hostile()).collect();
        let v: Vec<f64> = (0..n).map(|_| rng.hostile()).collect();
        for interp in [Interp::Step, Interp::Linear] {
            let s = series(&t, &v, interp);
            for _ in 0..4 {
                let x = rng.hostile();
                let y = s.at(x);
                if interp == Interp::Step {
                    assert!(
                        y.is_nan() || v.contains(&y),
                        "t = {t:?}, v = {v:?}, x = {x} -> {y}"
                    );
                }
                // Linear is checked for the interpolation property below, on
                // magnitudes where floating point can express the claim.
                let _ = y;
            }
        }
    }
}

/// The interpolation property, on values of ordinary magnitude: a blend never
/// leaves the hull of the samples, however scrambled the time master is.
///
/// The magnitudes are bounded deliberately. `v0 + (v1 - v0) · f` is the Java's
/// own expression, and at the extremes of the format it loses the property to
/// arithmetic rather than to logic — `1e-300 - 2.2e-16` rounds to `-2.2e-16`,
/// so a fraction of exactly 1 returns `0` instead of `1e-300`, and
/// `1e308 - (-1e308)` overflows to `inf`. Both are cancellation in a formula
/// that is shared with the Java and the TypeScript twin, not a port defect, and
/// pinning them would pin the wrong thing.
#[test]
fn a_linear_blend_never_leaves_the_hull_of_its_samples() {
    let mut rng = Lcg::new();
    for _ in 0..20_000 {
        let n = rng.below(9);
        let t: Vec<f64> = (0..n)
            .map(|_| (rng.below(2001) as f64) / 10.0 - 100.0)
            .collect();
        let v: Vec<f64> = (0..n)
            .map(|_| (rng.below(2001) as f64) / 10.0 - 100.0)
            .collect();
        let lo = v.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let s = series(&t, &v, Interp::Linear);
        for _ in 0..6 {
            let x = (rng.below(30_001) as f64) / 100.0 - 150.0;
            let y = s.at(x);
            assert!(
                y.is_nan() || (y >= lo && y <= hi),
                "t = {t:?}, v = {v:?}, x = {x} -> {y}, hull [{lo}, {hi}]"
            );
        }
    }
}

/// Why the fuzz above can assert a hull at all, on an array that is not sorted.
///
/// `lower_bound`'s loop leaves `lo == hi == lb`, and the *last* write to each
/// end pins a comparison: `lo` reached `lb` only through `lo = mid + 1` with
/// `t[lb-1] < x`, and `hi` reached `lb` only through `hi = mid` with
/// `t[lb] >= x`. (The two "never written" cases are `lb == 0` and `lb == n`,
/// and `at` handles both before the blend.) So `t0 < x <= t1` holds for the
/// bracketing pair **whatever order the array is in**, the interpolation
/// fraction is in `(0, 1]`, and the blend is a convex combination rather than
/// an extrapolation. This pins that claim on an array that is thoroughly out of
/// order.
#[test]
fn interpolation_on_a_scrambled_master_stays_between_its_bracketing_samples() {
    let t = [4.0, -9.0, 100.0, 0.5, -3.0, 7.0, 2.0];
    let v = [10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0];
    let s = series(&t, &v, Interp::Linear);
    let mut rng = Lcg::new();
    for _ in 0..5_000 {
        let x = (rng.below(40_000) as f64) / 100.0 - 200.0;
        let y = s.at(x);
        assert!(
            y.is_nan() || (10.0..=70.0).contains(&y),
            "at({x}) = {y} is outside the samples"
        );
    }
}

/// A *descending* master is the sub-case worth pinning, because the answer is
/// plausible and wrong and there is nothing this module can do about it.
///
/// `lower_bound` runs off the right-hand end (every sample compares below the
/// probe), so `at` takes the "past the last sample" branch and holds `v[n-1]` —
/// which on a reversed master is the value recorded *earliest*. It is not a
/// panic and it is not an extrapolation, and it is exactly what the Java and
/// `decimate.ts` do with the same array, so diverging here would desync the
/// three implementations for a file that is already corrupt. `SampledSeries`
/// documents the ascending precondition; this test is the record of what
/// breaking it buys.
#[test]
fn a_descending_time_master_answers_a_stored_sample_not_an_invention() {
    let v = [10.0, 20.0, 30.0, 40.0, 50.0];
    let s = series(&[5.0, 4.0, 3.0, 2.0, 1.0], &v, Interp::Linear);
    for x in [0.0, 1.5, 2.5, 3.5, 4.5, 6.0] {
        let y = s.at(x);
        assert!(y.is_nan() || v.contains(&y), "at({x}) = {y}");
    }
    assert_eq!(s.at(3.5), 50.0, "held from the right-hand end");
    // Not even an exact hit survives: `lower_bound` has already run off the
    // end of a reversed array, so the exact-hit branch is never reached.
    assert_eq!(s.at(5.0), 50.0);
}

/// The blend divides by `t[lb] - t[lb-1]`. Two samples sharing a timestamp are
/// the only way that is zero, and the branch is unreachable for them — but a
/// stalled master is common enough in real recordings that the claim is worth a
/// test rather than an argument.
#[test]
fn a_stalled_time_master_never_divides_by_a_zero_interval() {
    let mut rng = Lcg::new();
    for _ in 0..5_000 {
        // An ascending master with long runs of repeats.
        let n = 2 + rng.below(10);
        let mut t = Vec::with_capacity(n);
        let mut x = -3.0;
        for _ in 0..n {
            t.push(x);
            if rng.below(3) != 0 {
                x += 1.0;
            }
        }
        let v: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let s = series(&t, &v, Interp::Linear);
        for k in 0..n {
            for probe in [t[k], t[k] - 0.5, t[k] + 0.5] {
                let y = s.at(probe);
                assert!(
                    y.is_nan() || (y >= 0.0 && y <= (n - 1) as f64),
                    "t = {t:?}, probe {probe} -> {y}"
                );
            }
        }
    }
}

/// A `v` longer than `t` is the mirror of the case the port already documents.
#[test]
fn ragged_series_truncate_in_both_directions() {
    let long_values = SampledSeries::new(vec![0.0, 1.0], vec![1.0, 2.0, 3.0, 4.0], Interp::Linear);
    assert_eq!(long_values.at(0.5), 1.5);
    assert_eq!(long_values.at(9.0), 2.0);

    let no_values = SampledSeries::new(vec![0.0, 1.0], Vec::new(), Interp::Step);
    assert!(no_values.at(0.5).is_nan());
    assert_eq!(no_values.sample_on(&[0.0, 1.0]).len(), 2);

    let no_times = SampledSeries::new(Vec::new(), vec![1.0, 2.0], Interp::Step);
    assert!(no_times.at(0.0).is_nan());
}

// ────────────────────────────────────────────────────────────────────────────
// Envelope decimation — hostile ranges
// ────────────────────────────────────────────────────────────────────────────

/// `min_max` indexes `t` and `v` by arithmetic on caller-supplied indices. Every
/// combination of range and bucket count has to stay inside the arrays.
#[test]
fn every_window_of_every_array_stays_in_bounds() {
    let mut rng = Lcg::new();
    for _ in 0..20_000 {
        let n = rng.below(12);
        let t: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let v: Vec<f64> = (0..n).map(|_| rng.hostile()).collect();
        // Ranges deliberately drawn beyond the data, inverted, and enormous.
        let i0 = rng.below(20);
        let i1 = if rng.below(4) == 0 {
            usize::MAX - rng.below(3)
        } else {
            rng.below(20)
        };
        let buckets = match rng.below(5) {
            0 => 0,
            1 => usize::MAX,
            k => k,
        };
        let env = min_max(&t, &v, i0, i1, buckets);
        assert_eq!(env.min.len(), env.len());
        assert_eq!(env.max.len(), env.len());
        for &x in &env.t {
            assert!(t.contains(&x), "bucket time {x} is not a sample of {t:?}");
        }
    }
}

/// A `t`/`v` length mismatch reaches `min_max` from any source that breaks the
/// `ChannelData` contract; it must truncate, not index past the shorter array.
#[test]
fn a_ragged_channel_decimates_over_the_common_prefix() {
    let t = ramp(10, 1.0);
    let v = [0.0, 1.0, 2.0];
    let env = min_max(&t, &v, 0, 9, 4);
    assert_eq!(env.len(), 3);
    assert_eq!(env.min, vec![0.0, 1.0, 2.0]);

    let env = min_max(&t[..2], &v, 0, 9, 4);
    assert_eq!(env.len(), 2);

    assert!(min_max(&t, &[], 0, 9, 4).is_empty());
}

/// `lower_bound` is the one primitive every other module's index arithmetic
/// rests on: it must always land in `[0, len]`, whatever the array looks like.
#[test]
fn lower_bound_is_in_range_for_every_array_and_probe() {
    let mut rng = Lcg::new();
    for _ in 0..20_000 {
        let n = rng.below(10);
        let a: Vec<f64> = (0..n).map(|_| rng.hostile()).collect();
        let x = rng.hostile();
        assert!(lower_bound(&a, x) <= a.len(), "a = {a:?}, x = {x}");
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Raster construction
// ────────────────────────────────────────────────────────────────────────────

/// Every raster mode over the hostile pool: an answer or a typed error, and
/// never a raster bigger than the cap it was given.
#[test]
fn no_raster_argument_produces_a_raster_above_its_own_cap() {
    let mut rng = Lcg::new();
    for _ in 0..20_000 {
        let (t0, t1, dt) = (rng.hostile(), rng.hostile(), rng.hostile());
        let cap = [0u32, 1, 2, 10, 1000][rng.below(5)];
        match fixed(t0, t1, dt, cap) {
            Ok(r) => assert!(
                r.len() as u64 <= u64::from(cap),
                "fixed({t0}, {t1}, {dt}, {cap}) -> {} points",
                r.len()
            ),
            Err(MeasurementError::Parse(_) | MeasurementError::RasterCapExceeded { .. }) => {}
            Err(other) => panic!("fixed({t0}, {t1}, {dt}, {cap}) -> {other:?}"),
        }

        let base: Vec<f64> = (0..rng.below(6)).map(|_| rng.hostile()).collect();
        match union(&[&base[..]], cap) {
            Ok(r) => assert!(r.len() as u64 <= u64::from(cap), "union {base:?} cap {cap}"),
            Err(MeasurementError::Parse(_) | MeasurementError::RasterCapExceeded { .. }) => {}
            Err(other) => panic!("union({base:?}, {cap}) -> {other:?}"),
        }

        let dt = suggest_dt(t0, t1, cap);
        assert!(
            dt.is_nan() || dt > 0.0,
            "suggest_dt({t0}, {t1}, {cap}) = {dt}"
        );
    }
}

/// A time master whose samples are all `NaN` is a whole channel of gaps. It
/// must not collapse to one point — every `NaN` is a distinct sample, so the
/// point count stays honest and the cap still bites.
#[test]
fn an_all_nan_time_master_keeps_its_point_count() {
    let corrupt = vec![f64::NAN; 50];
    assert_eq!(union(&[&corrupt[..]], 100).unwrap().len(), 50);
    match union(&[&corrupt[..]], 10) {
        Err(MeasurementError::RasterCapExceeded {
            actual_points,
            suggested_dt,
            ..
        }) => {
            assert_eq!(actual_points, 50);
            assert!(suggested_dt.is_nan(), "got {suggested_dt}");
        }
        other => panic!("expected a cap error, got {other:?}"),
    }
}

/// Many bases, each large, is the shape that motivates merging — and the shape
/// whose concatenation is the biggest allocation in the module.
#[test]
fn a_wide_merge_is_bounded_by_the_cap_not_by_the_concatenation() {
    let base = ramp(20_000, 1e-4);
    let bases: Vec<&[f64]> = (0..64).map(|_| &base[..]).collect();
    // 1.28 M raw samples, 20 000 distinct times.
    assert_eq!(union(&bases, 20_000).unwrap().len(), 20_000);
    assert!(matches!(
        union(&bases, 19_999),
        Err(MeasurementError::RasterCapExceeded { .. })
    ));
}

/// A span of `inf - inf` is `NaN`, which slips past `t1 >= t0` and lands in the
/// point-count arithmetic. `(NaN + 1.0) as u64` saturates to **zero**, so
/// `fixed` used to answer an *empty* raster — a successful-looking answer to a
/// question with no answer, and the whole calculated signal came back as an
/// empty column with no diagnostic. It is reachable from a file, not only from
/// a caller: a decoded `f64` time master can carry an infinity.
#[test]
fn a_span_between_two_infinities_is_refused_rather_than_answered_empty() {
    for (t0, t1) in [
        (f64::INFINITY, f64::INFINITY),
        (f64::NEG_INFINITY, f64::NEG_INFINITY),
    ] {
        match fixed(t0, t1, 1.0, 1000) {
            Err(MeasurementError::Parse(m)) => assert!(m.contains("finite span"), "{m}"),
            other => panic!("fixed({t0}, {t1}) -> {other:?}"),
        }
    }
    // A span that is merely infinite still reports the cap, as documented.
    assert!(matches!(
        fixed(f64::NEG_INFINITY, f64::INFINITY, 1.0, 1000),
        Err(MeasurementError::RasterCapExceeded { .. })
    ));
}

// ────────────────────────────────────────────────────────────────────────────
// Calculated signals
// ────────────────────────────────────────────────────────────────────────────

/// `sameAs` hands the evaluator a channel's own time base verbatim, and nothing
/// requires a recorded master to ascend. `movavg` walks a window pointer
/// forward over that raster; on a descending one the pointer must not run past
/// the sample it is trailing.
#[test]
fn a_non_monotonic_raster_cannot_walk_the_moving_average_window_backwards() {
    let raster = [0.0, 100.0, 1.0, 2.0, -50.0, 3.0, f64::NAN, 4.0];
    let v: Vec<f64> = (0..raster.len()).map(|i| i as f64).collect();
    let inputs = bind(&[("x", series(&raster, &v, Interp::Step))]);
    for formula in ["movavg(x, 1)", "movavg(x, 1e-9)", "integral(x)", "delta(x)"] {
        let out = answered(formula, &raster, &inputs);
        assert!(out.starts_with("ok: 8"), "{formula} -> {out}");
    }
}

/// The four time operators against every degenerate window and raster the
/// wire can carry.
#[test]
fn the_time_operators_answer_on_every_degenerate_raster() {
    let rasters: [Vec<f64>; 6] = [
        Vec::new(),
        vec![0.0],
        vec![f64::NAN; 4],
        vec![f64::NEG_INFINITY, 0.0, f64::INFINITY],
        vec![0.0, 0.0, 0.0, 0.0],
        vec![-1e308, 1e308],
    ];
    for raster in &rasters {
        let v = vec![1.0; raster.len()];
        let inputs = bind(&[("x", series(raster, &v, Interp::Linear))]);
        for formula in [
            "delta(x)",
            "integral(x)",
            "movavg(x, 1)",
            "movavg(x, 1e308)",
            "delay(x, 1)",
            "delay(x, 0)",
            "integral(x) + movavg(x, 2) * delay(x, 1) - delta(x)",
        ] {
            let out = answered(formula, raster, &inputs);
            assert!(
                out.starts_with("ok:") || out.starts_with("eval:"),
                "{formula} on {raster:?} -> {out}"
            );
        }
    }
}

/// A window of `+inf` passes the `> 0` guard, so the trailing mean spans the
/// whole channel. Nothing about that is an error — but it must not be an
/// unbounded loop either.
#[test]
fn an_infinite_moving_average_window_spans_the_channel_once() {
    let raster = ramp(1000, 0.001);
    let v: Vec<f64> = (0..1000).map(|i| i as f64).collect();
    let inputs = bind(&[("x", series(&raster, &v, Interp::Step))]);
    let parsed = parse_formula("movavg(x, 1e999)").expect("1e999 lexes as a number");
    let out = evaluate(&parsed, &raster, &inputs).expect("an infinite window is not an error");
    // Trailing mean over everything seen so far: at the last point, the mean of
    // 0..999.
    assert_eq!(out[999], 499.5);
    assert_eq!(out[0], 0.0);
}

/// The parser's depth budget is the only thing between a hostile formula and a
/// stack overflow, and a stack overflow is an abort. Four recursive walks ride
/// on the tree the parser hands out — the time-op rewrite, the compiler, the
/// compiled tree's evaluation, and its `Drop` — and a `Call` at the root adds a
/// fifth by handing the whole subtree back to the general evaluator.
///
/// Run this in **debug** as well as release: `libtest` threads have a far
/// smaller stack than the main thread and far fatter frames, which is the
/// configuration the parser's budget was measured against.
#[test]
fn a_formula_at_the_parsers_depth_ceiling_evaluates_instead_of_aborting() {
    let raster = ramp(4, 1.0);
    let inputs = bind(&[("x", series(&raster, &[1.0, 2.0, 3.0, 4.0], Interp::Step))]);

    // Ten thousand parentheses is a refusal, not an abort — and the refusal
    // happens before the tree exists.
    let bomb = format!("{}x{}", "(".repeat(10_000), ")".repeat(10_000));
    let message = parse_formula(&bomb).unwrap_err().to_string();
    assert!(message.contains("too deeply nested"), "{message}");

    // Ten thousand terms, likewise.
    let chain = vec!["x"; 10_000].join(" + ");
    assert!(parse_formula(&chain).is_err());

    // The deepest tree the parser *will* hand out, with a call at the root so
    // that every point also re-enters `eval`. Found by bisection so it keeps
    // testing "whatever the parser admits" if the budget is retuned.
    let wrapped = |n: usize| format!("abs({})", vec!["x"; n].join(" + "));
    let mut lo = 1;
    let mut hi = 2;
    while parse_formula(&wrapped(hi)).is_ok() {
        lo = hi;
        hi *= 2;
        assert!(hi < 100_000, "the parser admits an unbounded chain");
    }
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if parse_formula(&wrapped(mid)).is_ok() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let out = answered(&wrapped(lo), &raster, &inputs);
    assert_eq!(out, "ok: 4 point(s)", "chain of {lo} inside a call");

    // And the deepest *nesting*, which is the heaviest shape there is.
    let mut lo = 1;
    let mut hi = 2;
    let nested = |n: usize| format!("{}x{}", "abs(".repeat(n), ")".repeat(n));
    while parse_formula(&nested(hi)).is_ok() {
        lo = hi;
        hi *= 2;
        assert!(hi < 100_000, "the parser admits unbounded nesting");
    }
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if parse_formula(&nested(mid)).is_ok() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let out = answered(&nested(lo), &raster, &inputs);
    assert_eq!(out, "ok: 4 point(s)", "nesting of {lo}");
}

/// `evaluate` materialises one raster-length column per input and holds them
/// all at once, so its working set is `raster × inputs` — a **product** of two
/// numbers a request sets independently and for free.
///
/// Measured before `MAX_INPUT_COLUMN_SAMPLES` existed, with a counting global
/// allocator: 8 inputs on a million-point raster peaked at 72 MB, 32 at 264 MB
/// and **128 at 1 032 MB**. Through the wasm boundary the same shape — 128
/// one-point inline inputs and `{"mode":"fixed","dt":1}` over their span — is a
/// **5 604-byte** request body and a **1 044 MB** peak, a 186 000×
/// amplification, and the allocation site is `sample_on`'s `collect()`: under
/// `panic = "abort"` that is the tab, not a diagnostic. Neither cap upstream
/// sees it. The boundary's `MAX_INPUT_SAMPLES` counts *input* samples, and 128
/// one-point series is 128 of them; `MAX_RASTER` counts the raster, and one
/// million is exactly what it allows.
///
/// After the fix the same call peaks at 132 MB and answers with a named
/// diagnostic. The test asserts both halves — that the wide case is refused and
/// that the case merging exists for is not.
#[test]
fn the_product_of_the_input_count_and_the_raster_length_is_bounded() {
    // Cheap stand-ins for the boundary's two caps; the shape is what matters,
    // not the byte count, so the raster is 1/8 of the real one and the input
    // count is scaled to match.
    let raster = ramp(125_000, 1.0);
    let one_point = |i: usize| {
        (
            format!("v{i}"),
            SampledSeries::new(vec![0.0], vec![1.0], Interp::Linear),
        )
    };

    // 8 columns + the output = 9 × 125 000 = 1.1 M samples. Well inside.
    let narrow: BTreeMap<String, SampledSeries> = (0..8).map(one_point).collect();
    let parsed = parse_formula("v0 + 1").expect("a trivial formula");
    assert_eq!(
        evaluate(&parsed, &raster, &narrow).map(|v| v.len()),
        Ok(125_000)
    );

    // 512 columns on the same raster is 64 M samples — the shape that was a
    // gigabyte at the boundary's own caps.
    let wide: BTreeMap<String, SampledSeries> = (0..512).map(one_point).collect();
    let message = evaluate(&parsed, &raster, &wide)
        .expect_err("512 full-length columns must be refused, not allocated")
        .to_string();
    // The remedy has to be in the message: which half of the product to shrink.
    assert!(message.contains("bind fewer signals"), "{message}");
    assert!(message.contains("coarser sample interval"), "{message}");

    // The case the merge exists for still evaluates: ten channels on one master
    // for ten seconds merge to a million points and eleven columns.
    let master = ramp(1_000_000, 1e-5);
    let ten: BTreeMap<String, SampledSeries> = (0..10)
        .map(|i| {
            (
                format!("c{i}"),
                SampledSeries::new(master.clone(), vec![1.0; master.len()], Interp::Step),
            )
        })
        .collect();
    let parsed = parse_formula("c0 + c9").expect("a trivial formula");
    assert_eq!(
        evaluate(&parsed, &master, &ten).map(|v| v.len()),
        Ok(1_000_000),
        "ten channels on one master is the workload the merge is for"
    );
}

/// The same product, reached from the *formula* rather than from the input
/// list: every `delta`/`integral`/`movavg`/`delay` in a formula is rewritten
/// into its own full-length synthetic column, and a formula may hold as many of
/// them as the parser's depth budget admits.
///
/// Measured before the fix: 200 `delta(x)` terms over a million-point raster
/// peaked at **1 616 MB** — from one bound input and 1.6 kB of formula text.
/// The columns are per *occurrence*, not per distinct expression, so the
/// repetition is the whole attack. `calc::MAX_SYNTHETIC_SAMPLES` is the guard;
/// this is the end-to-end check that it fires at the parser's own ceiling
/// rather than at some smaller shape.
#[test]
fn a_formula_full_of_time_operators_cannot_allocate_a_column_per_term() {
    let raster = ramp(125_000, 1.0);
    let v = vec![1.0; raster.len()];
    let inputs = bind(&[("x", series(&raster, &v, Interp::Linear))]);

    // A handful of synthetics is ordinary and must keep working.
    let few = answered(
        "delta(x) + integral(x) + movavg(x, 1) + delay(x, 1)",
        &raster,
        &inputs,
    );
    assert_eq!(few, "ok: 125000 point(s)");

    // The parser admits this shape; the evaluator must not build 200 columns
    // for it. Found by bisection so the test keeps meaning "whatever the parser
    // admits" if the depth budget is ever retuned.
    let chain = |n: usize| vec!["delta(x)"; n].join(" + ");
    let mut lo = 1;
    let mut hi = 2;
    while parse_formula(&chain(hi)).is_ok() {
        lo = hi;
        hi *= 2;
        assert!(hi < 100_000, "the parser admits an unbounded chain");
    }
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if parse_formula(&chain(mid)).is_ok() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    assert!(lo >= 128, "expected a deep chain to parse, got {lo} terms");
    let out = answered(&chain(lo), &raster, &inputs);
    assert!(
        out.starts_with("eval:") && out.contains("coarser sample interval"),
        "chain of {lo} delta terms -> {out}"
    );
    // "coarser sample interval" alone does not identify *which* ceiling fired —
    // `MAX_INPUT_COLUMN_SAMPLES` ends with the same remedy. This test is about
    // the formula half, so it has to name the formula half, or a future change
    // that deleted `MAX_SYNTHETIC_SAMPLES` could still pass here on the input
    // cap's message.
    assert!(
        out.contains("time operators") && out.contains("delta/integral/movavg/delay"),
        "the refusal must be the synthetic-column one, not the input one -> {out}"
    );
}

/// Formula text straight off the wire: unbalanced, unterminated, empty, and
/// full of bytes no keyboard produces. Every one is a diagnostic.
#[test]
fn hostile_formula_text_is_always_a_diagnostic() {
    let cases = [
        "",
        " ",
        "\0",
        "x +",
        "+ x",
        ")",
        "((((",
        "x ** 2",
        "x + 'unterminated",
        "delta(",
        "movavg(x,)",
        "\u{202e}x",
        "🙂",
        "x[",
        "1e999999999999999999999",
        "0x",
        "x = = 2",
        "and or not",
    ];
    for source in cases {
        match parse_formula(source) {
            Ok(expr) => {
                // If it parses it must also compile or refuse — never panic.
                let raster = ramp(3, 1.0);
                let inputs = bind(&[("x", series(&raster, &[1.0, 2.0, 3.0], Interp::Step))]);
                let _ = evaluate(&expr, &raster, &inputs);
                let _ = contains_call(&expr);
            }
            Err(e) => {
                let m = e.to_string();
                assert!(m.starts_with("Formula error:"), "{source:?} -> {m}");
            }
        }
    }
}

/// **Regression.** `pow10` deferred every decade outside `[-30, 30]` to
/// `libm::pow`, on the reasoning that the error there is an ULP and an ULP is
/// affordable. It is not an ULP. `libm::pow(10, -32)` lands one ULP *under*
/// 10⁻³², so the ladder's `1 * decade >= raw` test fails on the rung it should
/// have matched and the answer jumps to the next one — the same skipped rung
/// the literal table was introduced to close, one decade past its edge. 54
/// points of a decade grid took it before the fix.
///
/// Oracle: `MergedRaster.suggestDt(0, 1e-30, 101)` → `1.0E-32`,
/// `(0, 2e-30, 101)` → `2.0E-32`, `(0, 5e-30, 101)` → `5.0E-32`. This port
/// answered 2e-32, 5e-32 and 1e-31 — coarse by a factor of 2, 2.5 and 2.
#[test]
fn suggest_dt_does_not_skip_a_rung_below_the_literal_table() {
    assert_eq!(suggest_dt(0.0, 1e-30, 101).to_bits(), 1e-32f64.to_bits());
    assert_eq!(suggest_dt(0.0, 2e-30, 101).to_bits(), 2e-32f64.to_bits());
    assert_eq!(suggest_dt(0.0, 5e-30, 101).to_bits(), 5e-32f64.to_bits());
}

/// The general form of the same defect, at every decade the fdlibm lineage
/// `libm::pow` shares with `StrictMath.pow` gets wrong.
///
/// `suggest_dt(0, 10^k, 2)` is `10^k` exactly — `raw` is the whole span, and
/// the first rung matches it — so the returned bits *are* `pow10(k)`, and this
/// reads the private function through its only public consumer. All forty
/// spans below are decades where `StrictMath.pow(10, k)` differs from
/// `Double.parseDouble("1e" + k)` (checked: forty out of forty), while
/// `Math.pow` — what the reference actually runs — matches it at every one of
/// the 633 integer decades in `[-324, 308]`.
#[test]
fn suggest_dt_matches_the_oracle_on_the_decades_fdlibm_gets_wrong() {
    // Raw bits, so the expectation cannot be re-derived by the code under test.
    const DECADES: [u64; 40] = [
        0x0105_f1ca_8205_11c3, // 1e-303
        0x052d_bd86_cd62_38d9, // 1e-283
        0x0636_b0a8_e891_ffff, // 1e-278
        0x06d6_2884_f31e_93ff, // 1e-275
        0x0775_a391_d56b_dc87, // 1e-272
        0x0b9d_5384_4ee4_7dd1, // 1e-252
        0x10ce_5297_287c_2f45, // 1e-227
        0x14f4_8c22_ca71_a1bd, // 1e-207
        0x1a5a_8e90_f990_8e0d, // 1e-181
        0x1e17_08d0_f84d_3de7, // 1e-163
        0x2134_756c_cb01_abfb, // 1e-148
        0x255b_ba08_cf8c_979d, // 1e-128
        0x28e3_3d40_32c2_c7f5, // 1e-111
        0x2b95_df5c_a28e_f40d, // 1e-98
        0x2e13_e497_065c_d61f, // 1e-86
        0x3027_288e_1271_f513, // 1e-76
        0x3205_9165_a6dd_da5b, // 1e-67
        0x32da_53fc_9631_d10d, // 1e-63
        0x37d5_c72f_b155_2d83, // 1e-39
        0x3949_f623_d5a8_a733, // 1e-32
        0x46fe_d09b_ead8_7c03, // 1e34
        0x4807_8287_f49c_4a1d, // 1e39
        0x48a6_f578_c4e0_a061, // 1e42
        0x4a1b_5e7e_08ca_3a8f, // 1e49
        0x4bf9_7d4d_f19d_6057, // 1e58
        0x4c98_e45e_1df3_b015, // 1e61
        0x4e77_2eba_d6dd_c73d, // 1e70
        0x5a17_a2ec_c414_a03f, // 1e126
        0x5c2b_8434_22e3_a84d, // 1e136
        0x5d6a_3de0_4895_e46d, // 1e142
        0x6231_5d84_7ad0_0087, // 1e165
        0x66c2_62df_eebb_b0f9, // 1e187
        0x6b88_557f_3132_6bbb, // 1e210
        0x7019_c3bc_80c8_5c7f, // 1e232
        0x7336_e230_d05b_76cd, // 1e247
        0x75ea_03fd_e214_caf1, // 1e260
        0x789d_9388_b3aa_30a5, // 1e273
        0x7ae5_8504_1b2c_477f, // 1e284
        0x7cf9_0d56_b873_f4c7, // 1e294
        0x7e6d_dd4b_aa00_9303, // 1e301
    ];
    for bits in DECADES {
        let decade = f64::from_bits(bits);
        assert_eq!(
            suggest_dt(0.0, decade, 2).to_bits(),
            bits,
            "10^{} came back as {}",
            libm::log10(decade),
            suggest_dt(0.0, decade, 2)
        );
    }
}

/// **Regression.** `fixed` computed its point count as
/// `(floor((t1 - t0) / dt) + 1.0) as u64` — the `+ 1` in `f64`, where it is a
/// **no-op** once the quotient passes 2⁵³. The Java casts first
/// (`(long) Math.floor(x) + 1`), so the two answers part company at exactly the
/// scale where the count matters most: the message that refuses the raster.
///
/// The cap *decision* is unaffected (a count this large clears any `u32` cap
/// whichever way it rounds), which is why nothing else caught it — the number
/// is wrong only where it is being read, never where it is being compared.
///
/// Oracle: `fixed(0, 1e12, 1e-6, 1_000_000)` → 1 000 000 000 000 000 001 points
/// with `dt = 2000000.0`; `fixed(0, 1e18, 1, 1_000_000)` → the same count with
/// `dt = 2.0E12`; `fixed(0, 1e9, 1e-3, 1_000_000)` → 1 000 000 000 001, which
/// is under 2⁵³ and was already right.
#[test]
fn a_point_count_past_two_to_the_53_is_not_swallowed_by_its_own_plus_one() {
    let cases: [(f64, f64, f64, u64, f64); 3] = [
        (0.0, 1e12, 1e-6, 1_000_000_000_000_000_001, 2e6),
        (0.0, 1e18, 1.0, 1_000_000_000_000_000_001, 2e12),
        (0.0, 1e9, 1e-3, 1_000_000_000_001, 2e3),
    ];
    for (t0, t1, dt, points, dt_suggestion) in cases {
        match fixed(t0, t1, dt, 1_000_000) {
            Err(MeasurementError::RasterCapExceeded {
                actual_points,
                suggested_dt,
                cap,
            }) => {
                assert_eq!(actual_points, points, "fixed({t0}, {t1}, {dt})");
                assert_eq!(suggested_dt, dt_suggestion, "fixed({t0}, {t1}, {dt})");
                assert_eq!(cap, 1_000_000);
            }
            other => panic!("fixed({t0}, {t1}, {dt}) answered {other:?}"),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Formula width — the bound the depth budget never gave
// ────────────────────────────────────────────────────────────────────────────

/// `leaf` doubled `levels` times: a balanced tree `levels` deep with `2^levels`
/// leaves.
///
/// This is the shape the depth budget cannot see. `MAX_EXPR_DEPTH` is 256 and
/// 65 536 terms sit *sixteen* levels down, so the guard that stops a formula
/// being tall says nothing at all about one being wide — and every cost in
/// `calc.rs` is `nodes × something`.
fn balanced(leaf: &str, levels: usize) -> String {
    let mut src = leaf.to_string();
    for _ in 0..levels {
        src = format!("({src} + {src})");
    }
    src
}

/// The largest `k` for which `build(k)` parses.
fn parser_ceiling(build: impl Fn(usize) -> String) -> usize {
    let (mut lo, mut hi) = (1usize, 2usize);
    while parse_formula(&build(hi)).is_ok() {
        lo = hi;
        hi *= 2;
        assert!(hi < 200_000, "the parser admits an unbounded formula");
    }
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if parse_formula(&build(mid)).is_ok() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// A wide formula is refused, not evaluated.
///
/// Regression for a defect this audit found: `calc.rs` bounded a formula's
/// **depth** and nothing bounded its **node count**, so the three costs that
/// scale with nodes were all unbounded from one request field. Measured on this
/// port before `MAX_FORMULA_NODES` existed, in release:
///
/// * a 24 KB call-free formula (4096 terms) over the wasm boundary's own
///   million-point raster took **51 s**, and a megabyte of the same formula
///   about **fourteen minutes** — the worker wedged, with nothing able to
///   cancel it, from a request body smaller than a screenshot;
/// * a 90 KB formula took **6.2 s** to *compile*, on a **four-point** raster.
///   That one is not memory and not samples, so no byte-counting cap could ever
///   have caught it: every `Call` node built and sorted its own copy of the
///   whole slot table, making the compile `calls × slots` with both factors
///   growing together. It is fixed outright — `calc.rs::evaluate` builds the
///   table once — and bounded here as well;
/// * 1024 `movavg` calls in a 14 KB formula claimed **781 MB** of synthetic
///   columns, and a megabyte of them 52 GB, which on wasm32 is an allocator
///   trap and therefore an abort rather than a diagnostic.
///
/// The refusals below are all sub-second; the assertion is the refusal, and the
/// wall-clock bound is the standing house rule that every entry point *decides*
/// in bounded time.
#[test]
fn a_wide_formula_is_refused_instead_of_wedging_the_worker() {
    let raster = ramp(1000, 1e-3);
    let v: Vec<f64> = (0..1000).map(|i| i as f64).collect();
    let inputs = bind(&[("x", series(&raster, &v, Interp::Linear))]);

    // 65 536 terms — 393 kB of formula text, sixteen levels deep, so the depth
    // budget passes it without complaint.
    let start = std::time::Instant::now();
    let message = parse_formula(&balanced("x", 16))
        .expect_err("65 536 terms must be refused")
        .to_string();
    let elapsed = start.elapsed();
    assert!(message.contains("terms to evaluate"), "{message}");
    assert!(
        elapsed < std::time::Duration::from_secs(30),
        "deciding a 393 kB formula took {elapsed:?}"
    );

    // The compile-time quadratic: every leaf is both a call and a time operator,
    // so `calls × slots` grows as the square. Refused before either factor can.
    let message = parse_formula(&balanced("abs(movavg(x, 1))", 12))
        .expect_err("4096 call+time-op leaves must be refused")
        .to_string();
    assert!(message.contains("terms to evaluate"), "{message}");

    // Wide via one argument list rather than via nesting — the argList costs no
    // depth at all, so this shape is two levels deep and 3073 nodes.
    let args = vec!["movavg(x, 1)"; 1024].join(", ");
    let message = parse_formula(&format!("abs({args})"))
        .expect_err("1024 arguments must be refused")
        .to_string();
    assert!(message.contains("terms to evaluate"), "{message}");

    // `evaluate` is `pub`, so the guard cannot live only in `parse_formula`: an
    // `Expr` assembled by any other route has to be refused too.
    let wide = Expr::call("abs", vec![Expr::var("x"); 2000]);
    let message = evaluate(&wide, &raster, &inputs)
        .expect_err("a hand-built wide tree must be refused")
        .to_string();
    assert!(message.contains("terms to evaluate"), "{message}");

    // …and a formula of ordinary width still evaluates.
    assert_eq!(
        answered(&balanced("x", 4), &raster, &inputs),
        "ok: 1000 point(s)"
    );
}

/// The node budget must not shrink what the *depth* budget already admits.
///
/// The two guards have to be ordered: a formula the parser hands out must still
/// evaluate, or the width bound has quietly deleted the tall one. Measured
/// ceilings, found by bisection here so this keeps testing "whatever the parser
/// admits" if either budget is retuned: 249 terms bare, 245 inside a call, 62
/// levels of `abs(` nesting — ~500 nodes at the worst, against a budget of 1024.
#[test]
fn the_deepest_formula_the_parser_admits_still_evaluates() {
    let raster = ramp(4, 1.0);
    let inputs = bind(&[("x", series(&raster, &[1.0, 2.0, 3.0, 4.0], Interp::Step))]);

    let chain = parser_ceiling(|k| vec!["x"; k].join(" + "));
    assert!(chain >= 128, "expected a long chain to parse, got {chain}");
    assert_eq!(
        answered(&vec!["x"; chain].join(" + "), &raster, &inputs),
        "ok: 4 point(s)",
        "bare chain of {chain} terms"
    );

    let wrapped = parser_ceiling(|k| format!("abs({})", vec!["x"; k].join(" + ")));
    assert_eq!(
        answered(
            &format!("abs({})", vec!["x"; wrapped].join(" + ")),
            &raster,
            &inputs
        ),
        "ok: 4 point(s)",
        "chain of {wrapped} terms inside a call"
    );

    let nested = parser_ceiling(|k| format!("{}x{}", "abs(".repeat(k), ")".repeat(k)));
    assert_eq!(
        answered(
            &format!("{}x{}", "abs(".repeat(nested), ")".repeat(nested)),
            &raster,
            &inputs
        ),
        "ok: 4 point(s)",
        "nesting of {nested}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// The moving average's running sum
// ────────────────────────────────────────────────────────────────────────────

/// A trailing mean recovers once the offending sample has left the window.
///
/// Regression for a silent wrong answer this audit found. `movavg` slides its
/// window by adding the entering sample and *subtracting* the leaving one, and
/// once the accumulator has been ±∞ that never recovers: `∞ - x` is `∞` and
/// `∞ - ∞` is `NaN`. So one bad sample in a 500 000-point channel turned every
/// later point into a gap — over data that was perfectly good, and with no
/// signal that anything had happened. This module's headline rule is that a gap
/// is never bridged; manufacturing gaps out of one is that rule broken the other
/// way, and `NaN` is the worst possible disguise for it, because it renders as
/// "the instrument recorded nothing here".
///
/// Java does the same, and `measurement_parity.rs` records the divergence
/// deliberately rather than inheriting it.
#[test]
fn a_moving_average_recovers_once_a_non_finite_sample_leaves_the_window() {
    let raster = ramp(10, 1.0);
    let mean = |v: &[f64]| {
        let inputs = bind(&[("x", series(&raster, v, Interp::Step))]);
        evaluate(&parse_formula("movavg(x, 2)").unwrap(), &raster, &inputs).expect("a mean")
    };

    // One `+∞`, then ordinary numbers. Before the fix: [∞, ∞, NaN × 8].
    let mut v = vec![1.0; 10];
    v[0] = f64::INFINITY;
    let out = mean(&v);
    assert!(out[0].is_infinite() && out[2].is_infinite(), "{out:?}");
    assert_eq!(
        &out[3..],
        &[1.0; 7],
        "the window past t = 3 holds only 1.0s: {out:?}"
    );

    // One `−∞` in the middle. Before the fix the tail was NaN from t = 6 on.
    let mut v = vec![1.0; 10];
    v[3] = f64::NEG_INFINITY;
    let out = mean(&v);
    assert_eq!(&out[0..3], &[1.0, 1.0, 1.0], "{out:?}");
    assert!(out[3..6].iter().all(|x| *x == f64::NEG_INFINITY), "{out:?}");
    assert_eq!(&out[6..], &[1.0; 4], "{out:?}");

    // Both signs: the `NaN` while they share a window is genuine — `∞ + (−∞)`
    // has no value — and it must not outlive them.
    let mut v = vec![1.0; 10];
    v[0] = f64::INFINITY;
    v[1] = f64::NEG_INFINITY;
    let out = mean(&v);
    assert!(out[1].is_nan() && out[2].is_nan(), "{out:?}");
    assert_eq!(&out[4..], &[1.0; 6], "{out:?}");

    // Two adjacent `1e308` — **finite**, an ordinary `f64` a float channel can
    // carry — overflow the sum. Before the fix every later point was `∞`.
    let mut v = vec![1.0; 10];
    v[0] = 1e308;
    v[1] = 1e308;
    let out = mean(&v);
    assert!(out[3].is_finite(), "the overflow must not persist: {out:?}");

    // **The limit, pinned rather than papered over.** The repair fires on a
    // non-finite accumulator; plain cancellation leaves it perfectly finite and
    // is therefore undetectable. `1e308 + 1 - 1e308` is `0`, so the tail above
    // reports zero where the truth is one. Java has it too, and the only cure —
    // a window sum that never subtracts — changes the summation order and so
    // changes the answer on *every* ordinary channel. Fixing this is a
    // numerics decision, not a robustness one.
    assert_eq!(&out[4..], &[0.0; 6], "the documented cancellation limit");
    let mut v = vec![1.0; 10];
    v[0] = 1e300;
    let out = mean(&v);
    assert_eq!(
        out[3], 0.0,
        "same cancellation without any overflow: {out:?}"
    );
}

/// The repair is bounded, so a channel engineered to overflow at *every* point
/// cannot turn an O(n) pass into O(n·window).
///
/// A whole-channel window over 200 000 identical `1e308` samples makes every
/// window's true sum overflow, so every recompute fails and would be attempted
/// again at the next point. The budget is four passes over the channel, after
/// which the running sum is reported as-is — `+∞`, which is the true answer
/// here, and never a fabricated gap.
#[test]
fn the_moving_average_repair_cannot_become_quadratic() {
    let n = 200_000usize;
    let raster = ramp(n, 1e-3);
    let inputs = bind(&[("x", series(&raster, &vec![1e308; n], Interp::Step))]);
    let start = std::time::Instant::now();
    let out = evaluate(&parse_formula("movavg(x, 1e9)").unwrap(), &raster, &inputs)
        .expect("an infinite window is not an error");
    let elapsed = start.elapsed();
    assert!(
        out[n - 1].is_infinite() && out[n - 1] > 0.0,
        "{}",
        out[n - 1]
    );
    assert!(
        elapsed < std::time::Duration::from_secs(30),
        "an all-overflow channel took {elapsed:?}"
    );
}

/// The recovery must not depend on the window being tiny.
///
/// **Regression for a defect in the first fix**, found by verifying it rather
/// than by trusting it. That fix recomputed the window sum on *every* non-finite
/// accumulator, charged against a budget of four passes over the channel — and
/// the points where the offending sample is **still inside** the window are the
/// ones it spent the budget on, even though a recompute there is arithmetically
/// guaranteed to come back `±∞` again. Those hopeless passes cost `Σ span`,
/// which is `W²/2` for a window of `W` raster points, so the budget ran out
/// whenever `W > √(8n)` — and the useful repair, the one after the sample
/// leaves, never happened.
///
/// [`a_moving_average_recovers_once_a_non_finite_sample_leaves_the_window`] did
/// not see it because a ten-point raster with a 2 s window holds *two* samples.
/// A real one does not: measured on this port against that fix, one `+∞` at the
/// head of a 200 000-point channel with a 2 s window at 1 kHz left **195 998 of
/// the remaining 195 998 points `NaN`** — the whole rest of the recording, which
/// is precisely the defect that fix was written to close. A 10 s window over the
/// same channel, and a 300 s window over a 1000-point one, failed the same way.
///
/// The cure is to gate the repair on the window's ±∞ population rather than on
/// the accumulator, so the hopeless passes are never attempted; see
/// `calc.rs::movavg`. What is left is `O(n)` in total, which is why this case
/// needs no budget at all and cannot be starved by any other.
#[test]
fn a_moving_average_recovers_over_a_window_of_realistic_width() {
    // 1 kHz for 200 s, one `+∞` at the head, a 2 s trailing window: 2000 raster
    // points inside it, against a budget-based repair's ~1265-point ceiling.
    let n = 200_000usize;
    let raster = ramp(n, 1e-3);
    let mut v = vec![1.0; n];
    v[0] = f64::INFINITY;
    let inputs = bind(&[("x", series(&raster, &v, Interp::Step))]);
    let out = evaluate(&parse_formula("movavg(x, 2)").unwrap(), &raster, &inputs).expect("a mean");
    let tail = &out[4_002..];
    assert_eq!(
        tail.iter().filter(|x| x.is_nan()).count(),
        0,
        "{} of {} points past the window are a fabricated gap",
        tail.iter().filter(|x| x.is_nan()).count(),
        tail.len()
    );
    assert_eq!(out[n - 1], 1.0, "the last point of a channel of 1.0s");

    // A window wider than the whole budget, and an `−∞` in the middle rather
    // than at the head: 10 s at 1 kHz is 10 000 points.
    let mut v = vec![2.0; n];
    v[100_000] = f64::NEG_INFINITY;
    let inputs = bind(&[("x", series(&raster, &v, Interp::Step))]);
    let out = evaluate(&parse_formula("movavg(x, 10)").unwrap(), &raster, &inputs).expect("a mean");
    assert!(
        out[100_000].is_infinite() && out[100_000] < 0.0,
        "the sample itself is genuinely −∞: {}",
        out[100_000]
    );
    assert_eq!(out[n - 1], 2.0, "the channel recovers: {}", out[n - 1]);
    assert_eq!(
        out[120_000..].iter().filter(|x| x.is_nan()).count(),
        0,
        "gaps fabricated after the −∞ left the window"
    );

    // Short channel, wide window — the ratio is what matters, not the length.
    let raster = ramp(1_000, 1.0);
    let mut v = vec![1.0; 1_000];
    v[0] = f64::INFINITY;
    let inputs = bind(&[("x", series(&raster, &v, Interp::Step))]);
    let out =
        evaluate(&parse_formula("movavg(x, 300)").unwrap(), &raster, &inputs).expect("a mean");
    assert_eq!(&out[700..], &[1.0; 300], "a 300 s window over 1000 points");
}
