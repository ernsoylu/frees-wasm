//! Numeric-parity sweep of the Phase 10 measurement surface, against the Java
//! it was ported from.
//!
//! Companion to [`tests/measurement_robustness.rs`](measurement_robustness.rs),
//! which asks whether every entry point *answers*. This file asks whether it
//! answers with the **same number** — a different failure mode, and a quieter
//! one: a plausible wrong value in a calculated signal is indistinguishable
//! from data until someone reruns the analysis on the old stack.
//!
//! Every expected value below was **produced by running the reference classes**
//! — `SampledSeries`, `EnvelopeDecimator`, `MergedRaster`, `TimeSeriesEvaluator`
//! out of `../frEES/backend/core/build/classes/java/main`, driven by a probe
//! program on this machine — not derived by reading the Java source. That
//! distinction earned its keep: four of the tables here contradict what a
//! careful reading of the code predicted, most sharply
//! [`a_stalled_or_corrupt_time_master_gives_the_javas_answers`], where a `NaN`
//! in the time master corrupts the binary search rather than the arithmetic.
//! Where the Java prints an unlovely number (`4.9999999999999995E-11`,
//! `2.0999999999999996`, `17.249999999999996`) that number is reproduced
//! verbatim, because agreeing on the pretty cases and diverging on the ugly
//! ones is precisely what this file exists to catch.
//!
//! The sweep compared about 21 000 values across four surfaces: 4 896
//! `suggestDt` calls covering every decade in `[-25, 25]` at every rung of the
//! 1-2-5 ladder and one ULP either side of it; 847 window slices; 14 319
//! per-sample results from 43 formulas over a channel pair carrying gaps,
//! infinities and zeros; and the full branch table of `at`, `lowerBound` and
//! `minMax`. **Four divergences survived scrutiny, all four are fixed, and each
//! has its regression in the first section below.**
//!
//! * [`the_decade_of_a_suggested_dt_is_not_one_ulp_short`] — `suggest_dt` built
//!   its ladder on `libm::pow(10.0, k)`, which is *not* correctly rounded: it
//!   is one ULP out at eight of the sixty-one decades in `[-30, 30]`, where
//!   Java's `Math.pow` is exact at every one (checked against
//!   `Double.parseDouble("1e" + k)`). Two of those errors point downwards, and
//!   the ladder tests `1 * decade >= raw` — so at `k = -5` and `k = -17` the
//!   rung that should have matched failed and the answer jumped to the next.
//!   Measured: `suggest_dt(0, 1e-4, 11)` returned **2 × 10⁻⁵ where the Java
//!   says 10⁻⁵**. That number is rendered on the frontend's "use this dt
//!   instead" button, so the user was being offered half the resolution they
//!   were entitled to, spelled unroundably. (`raster.rs::pow10`)
//! * [`a_guarded_call_is_not_evaluated_where_its_guard_is_false`] — the Java's
//!   compiled `and`/`or` *are* Java's `&&`/`||`, which short-circuit; this port
//!   evaluated both operands. Invisible until the right operand fails — and
//!   the entire reason to write `p > 0 and enthalpy(…)` over measured data is
//!   that the property call is undefined exactly where the guard is false.
//!   Measured: `x > 5 and nosuchfn(x) > 0` returned `[0, 0, 0]` from the Java
//!   and failed the whole channel here. (`calc.rs::Compiled::Logical`)
//! * [`a_gap_in_the_exponent_stays_a_gap`] — `^` was C's `pow`, which answers
//!   `1` for `pow(1, NaN)` and `pow(±1, ±∞)` where Java's `Math.pow` answers
//!   `NaN`. A `NaN` exponent is a dropout in the exponent channel, so C's
//!   answer invents a sample nobody recorded wherever the base sits at exactly
//!   1 — breaking this module's headline rule, not merely its parity.
//!   (`calc.rs::java_pow`)
//! * [`a_gap_in_the_exponent_stays_a_gap_inside_a_call_argument`] — found while
//!   *verifying* the entry above, which fixed `^` only in the compiled calc
//!   tree. A function call is not compiled: the whole subtree goes to the
//!   document evaluator, which had C's `pow` too. So `abs(b ^ e)` re-invented
//!   the same `1.0` the line above had just removed, and `abs(b ^ inf)` was
//!   wrong at *every* sample rather than only at the gap. Java's
//!   `ast/Evaluator` uses `Math.pow` exactly as `TimeSeriesEvaluator` does, so
//!   this was one rule with two sites. (`eval.rs::apply_binop`)
//!
//! What held, having been attacked hard enough that saying so is worth
//! something: `at` on every branch in both interpolation modes; `lower_bound`
//! including `NaN` probes and runs of equal timestamps; `min_max`'s bucket
//! edges and its **midpoint-index** representative time at every bucket count
//! from one to past the sample count, and at three million samples; the
//! trapezoid `integral`, `delta`, the trailing-window `movavg` (including which
//! side of `t - window` is inclusive) and `delay` — over regular rasters,
//! irregular ones, rasters with duplicated timestamps, rasters with a `NaN`
//! master, and gaps and infinities in the values; `union`'s sort/dedupe
//! including signed zero and `NaN`; and `fixed`'s accumulation drift.
//!
//! Three divergences are **known and left in place**, and they are one family:
//! inside a function-call argument the calc path defers to the document
//! evaluator, which *refuses* three things the Java's `ast/Evaluator` answers —
//! division by zero (`±∞`), a negative base raised to a non-integer power
//! (`NaN`), and zero raised to a negative power (`+∞`). Those guards are
//! engine-wide and load-bearing for Newton, so they are out of this module's
//! reach; [`division_by_zero_inside_a_call_still_diverges_from_the_java`] and
//! [`the_document_evaluators_power_guards_still_diverge_from_the_java`] pin
//! them as a record rather than an aspiration. Note the shape they share with
//! the fixed entries above and *not* with each other: a guard fails loudly,
//! where C's `pow` returned a plausible wrong number.
//!
//! Also measured and *not* treated as a defect: `libm::pow` and the JVM's
//! `Math.pow` intrinsic disagree by up to one ULP on ordinary arguments (3 of
//! the 43 sweep formulas, all of them `^`). State the shape of that honestly,
//! because it is a trade and not a tie: on every case checked where the two
//! disagree — `2^-0.5`, `0.5^-0.5`, `10^-5`, `10^-17` — **this host's own
//! `f64::powf` agrees with the JVM, not with `libm`**, `Math.pow` being
//! correctly rounded where the fdlibm lineage is not. So `libm` is the least
//! accurate of the three, and it is chosen anyway for one reason: it is the
//! only one that makes a native run and a wasm run agree bit for bit, which is
//! a crate-wide rule. Accuracy is what is being traded, deliberately. The
//! `java_pow` rules above are not in that category — they are a *different
//! answer*, not a nearer one — which is why only they were reproduced.

use std::collections::BTreeMap;

use frees_core::measurement::calc::{evaluate, parse_formula};
use frees_core::measurement::decimate::{lower_bound, min_max};
use frees_core::measurement::raster::{fixed, suggest_dt, union};
use frees_core::measurement::series::{Interp, SampledSeries};

const NAN: f64 = f64::NAN;
const INF: f64 = f64::INFINITY;

// ── helpers ─────────────────────────────────────────────────────────────────

fn series(t: &[f64], v: &[f64], interp: Interp) -> SampledSeries {
    SampledSeries::new(t.to_vec(), v.to_vec(), interp)
}

fn bind(pairs: &[(&str, SampledSeries)]) -> BTreeMap<String, SampledSeries> {
    pairs
        .iter()
        .map(|(name, s)| ((*name).to_string(), s.clone()))
        .collect()
}

fn one(t: &[f64], v: &[f64], interp: Interp) -> BTreeMap<String, SampledSeries> {
    bind(&[("x", series(t, v, interp))])
}

fn run(formula: &str, raster: &[f64], inputs: &BTreeMap<String, SampledSeries>) -> Vec<f64> {
    let parsed = parse_formula(formula).unwrap_or_else(|e| panic!("`{formula}`: {e}"));
    evaluate(&parsed, raster, inputs).unwrap_or_else(|e| panic!("`{formula}`: {e}"))
}

fn run_err(formula: &str, raster: &[f64], inputs: &BTreeMap<String, SampledSeries>) -> String {
    let parsed = parse_formula(formula).unwrap_or_else(|e| panic!("`{formula}`: {e}"));
    match evaluate(&parsed, raster, inputs) {
        Ok(v) => panic!("`{formula}` should have failed, got {v:?}"),
        Err(e) => e.to_string(),
    }
}

/// Bit-for-bit against the oracle, with `NaN` matching `NaN` — its payload and
/// sign are not observable, because the wasm boundary renders every non-finite
/// as JSON `null`. Everything else compares by bits, so `0.1 + 0.2` cannot pass
/// for `0.3` and `-0.0` cannot pass for `0.0`.
#[track_caller]
fn same(got: &[f64], want: &[f64], what: &str) {
    assert_eq!(got.len(), want.len(), "{what}: length {got:?} vs {want:?}");
    for (i, (&g, &w)) in got.iter().zip(want).enumerate() {
        let ok = if w.is_nan() {
            g.is_nan()
        } else {
            g.to_bits() == w.to_bits()
        };
        assert!(
            ok,
            "{what}[{i}]: got {g:?}, Java gives {w:?}\n  got  {got:?}\n  java {want:?}"
        );
    }
}

// ════════════════════════════════════════════════════════════════════════════
// The three divergences this sweep found
// ════════════════════════════════════════════════════════════════════════════

/// `libm::pow(10.0, k)` is one ULP low at `k = -5`, and the ladder's `>=` test
/// turns that into a whole missed rung.
///
/// The table is the oracle's, at the eight decades where the fdlibm lineage
/// disagrees with `Math.pow`: `k` = −29, −24, −21, −20, −17, −11, −5, 29. Three
/// of the rung-5 answers are untidy — they come from `5.0 * decade` rounding,
/// and reproducing *those* is the real test, since matching only the round ones
/// would pass with the defect still in place.
#[test]
fn the_decade_of_a_suggested_dt_is_not_one_ulp_short() {
    // The headline case. Before the fix this returned 2e-5.
    assert_eq!(suggest_dt(0.0, 1e-4, 11), 1e-5);
    // The other downward error, at k = -17.
    assert_eq!(suggest_dt(0.0, 1e-16, 11), 1e-17);

    // decade, then the rung-1 / rung-2 / rung-5 answers at that decade.
    let cases: &[(f64, f64, f64, f64)] = &[
        (1e-29, 1e-29, 2e-29, 1e-28),
        (1e-24, 1e-24, 2e-24, 5e-24),
        (1e-21, 1e-21, 2e-21, 5e-21),
        (1e-20, 1e-20, 2e-20, 1e-19),
        (1e-17, 1e-17, 2e-17, 5.000_000_000_000_000_5e-17),
        (1e-11, 1e-11, 2e-11, 4.999_999_999_999_999_5e-11),
        (1e-5, 1e-5, 2e-5, 5e-5),
        (1e29, 1e29, 2e29, 4.999_999_999_999_999_4e29),
    ];
    for &(decade, rung1, rung2, rung5) in cases {
        // cap = 11 makes `raw` exactly `span / 10`, so the span lands on a rung.
        same(&[suggest_dt(0.0, decade * 10.0, 11)], &[rung1], "rung 1");
        same(
            &[suggest_dt(0.0, 2.0 * decade * 10.0, 11)],
            &[rung2],
            "rung 2",
        );
        same(
            &[suggest_dt(0.0, 5.0 * decade * 10.0, 11)],
            &[rung5],
            "rung 5",
        );
    }

    // The property the whole cap story rests on, walked over the same ladder
    // the sweep did: the suggestion must yield a raster that actually fits.
    // The Java has zero violations here; so must this.
    let mut violations = 0;
    for k in -20i32..=20 {
        for m in [1.0, 1.0001, 2.0, 2.5, 5.0, 7.0, 9.99] {
            for cap in [2u32, 3, 11, 100, 1000, 2400, 100_000, 1_000_000] {
                let span = m * libm::pow(10.0, f64::from(k)) * (f64::from(cap) - 1.0);
                let dt = suggest_dt(0.0, span, cap);
                if (libm::floor(span / dt) + 1.0) as u64 > u64::from(cap) {
                    violations += 1;
                }
            }
        }
    }
    assert_eq!(violations, 0, "a suggested dt did not fit its own cap");
}

/// The Java's compiled `and`/`or` short-circuit, which is what makes a property
/// call *guardable* over measured data.
///
/// Oracle, on `x = [0, 2, 0]` at `t = [0, 1, 2]`:
///
/// ```text
/// x > 5 and nosuchfn(x) > 0   ->  [0.0, 0.0, 0.0]
/// x < 5 or  nosuchfn(x) > 0   ->  [1.0, 1.0, 1.0]
/// x > 1 and nosuchfn(x) > 0   ->  ERR Formula failed at t = 1.0
/// x < 1 or  nosuchfn(x) > 0   ->  ERR Formula failed at t = 1.0
/// nosuchfn(x) > 0 and x > 5   ->  ERR Formula failed at t = 0.0
/// ```
#[test]
fn a_guarded_call_is_not_evaluated_where_its_guard_is_false() {
    let raster = [0.0, 1.0, 2.0];
    let inputs = one(&raster, &[0.0, 2.0, 0.0], Interp::Step);

    // A guard that is false at every sample means the call never runs at all.
    // Before the fix both of these failed the entire channel.
    same(
        &run("x > 5 and nosuchfn(x) > 0", &raster, &inputs),
        &[0.0, 0.0, 0.0],
        "and, guard false everywhere",
    );
    same(
        &run("x < 5 or nosuchfn(x) > 0", &raster, &inputs),
        &[1.0, 1.0, 1.0],
        "or, guard true everywhere",
    );

    // A guard true at exactly one sample reports *that* sample's timestamp.
    // This is what pins short-circuiting rather than mere error-swallowing:
    // without it the failure is reported at t = 0, the first sample.
    for f in ["x > 1 and nosuchfn(x) > 0", "x < 1 or nosuchfn(x) > 0"] {
        let m = run_err(f, &raster, &inputs);
        assert!(m.contains("t = 1"), "{f}: {m}");
    }
    // A call on the *left* is never skipped, so it still reports t = 0.
    let m = run_err("nosuchfn(x) > 0 and x > 5", &raster, &inputs);
    assert!(m.contains("t = 0"), "{m}");

    // Short-circuiting must not move a value anywhere. `and`/`or`/`not` over
    // gap-bearing data, against the Java, sample for sample. Note that a gap is
    // *truthy*: the test on both sides is `!= 0.0`, and `NaN != 0.0`. That is
    // surprising, it is parity, and it is what makes `a and c` differ from
    // `c and a` in the presence of a failing call.
    let t = [0.0, 1.0, 2.0, 3.0];
    let g = bind(&[
        ("a", series(&t, &[1.0, NAN, 0.0, -1.0], Interp::Step)),
        ("c", series(&t, &[0.0, 1.0, NAN, 1.0], Interp::Step)),
    ]);
    same(
        &run("a and c", &t, &g),
        &[0.0, 1.0, 0.0, 1.0],
        "and over gaps",
    );
    same(
        &run("a or c", &t, &g),
        &[1.0, 1.0, 1.0, 1.0],
        "or over gaps",
    );
    same(
        &run("not a", &t, &g),
        &[0.0, 0.0, 1.0, 0.0],
        "not over gaps",
    );
    same(
        &run("a and c or not a", &t, &g),
        &[0.0, 1.0, 1.0, 1.0],
        "mixed connectives",
    );

    // The skipped operand still has to be *dropped*, and it is an arbitrarily
    // deep tree that was never walked during evaluation.
    same(
        &run("x > 99 and (x + x + x)", &raster, &inputs),
        &[0.0, 0.0, 0.0],
        "guarded chain",
    );
}

/// `Math.pow` is not C's `pow`, and both places they differ amount to a gap
/// being silently filled in.
///
/// Oracle: `b ^ e` over the twelve `(base, exponent)` pairs below gives
/// `[NaN, NaN, NaN, 1.0, NaN, NaN, NaN, INF, 0.0, INF, 1.0, -8.0]`.
#[test]
fn a_gap_in_the_exponent_stays_a_gap() {
    let pairs: [(f64, f64); 12] = [
        (1.0, NAN),
        (-1.0, NAN),
        (2.0, NAN),
        (NAN, 0.0),
        (1.0, INF),
        (-1.0, INF),
        (1.0, -INF),
        (2.0, INF),
        (0.5, INF),
        (0.0, -1.0),
        (0.0, 0.0),
        (-2.0, 3.0),
    ];
    let t: Vec<f64> = (0..pairs.len()).map(|i| i as f64).collect();
    let bases: Vec<f64> = pairs.iter().map(|p| p.0).collect();
    let exps: Vec<f64> = pairs.iter().map(|p| p.1).collect();
    let inputs = bind(&[
        ("b", series(&t, &bases, Interp::Step)),
        ("e", series(&t, &exps, Interp::Step)),
    ]);
    same(
        &run("b ^ e", &t, &inputs),
        &[NAN, NAN, NAN, 1.0, NAN, NAN, NAN, INF, 0.0, INF, 1.0, -8.0],
        "b ^ e",
    );

    // The reachable shape, spelled out: a boolean channel sitting high, raised
    // to a channel with a dropout. Before the fix the gap rendered as 1.0 — a
    // sample the recording does not contain, at full plausibility.
    let t = [0.0, 1.0, 2.0];
    let inputs = bind(&[
        ("high", series(&t, &[1.0, 1.0, 1.0], Interp::Step)),
        ("ratio", series(&t, &[2.0, NAN, 0.5], Interp::Step)),
    ]);
    let out = run("high ^ ratio", &t, &inputs);
    assert!(out[1].is_nan(), "a gap must survive `^`: {out:?}");
    same(&out, &[1.0, NAN, 1.0], "high ^ ratio");
}

// ════════════════════════════════════════════════════════════════════════════
// SampledSeries::at, branch by branch
// ════════════════════════════════════════════════════════════════════════════

/// The oracle's full branch table for one series that visits every arm: before
/// the first sample, an exact hit, a hit on a stored `NaN`, a blend, a blend
/// poisoned at each end, past the last sample, and a `NaN` query.
#[test]
fn at_matches_the_java_on_every_branch_of_both_modes() {
    let t = [-1.5, 0.0, 0.25, 3.0, 3.5, 10.0];
    let v = [-2.0, 0.0, 7.5, NAN, 1.0, -1e9];
    let probes = [
        -2.0, -1.5, -1.0, 0.0, 0.1, 0.25, 1.0, 2.9999, 3.0, 3.25, 3.5, 7.0, 10.0, 11.0, NAN,
    ];

    let step = series(&t, &v, Interp::Step);
    let got: Vec<f64> = probes.iter().map(|&p| step.at(p)).collect();
    same(
        &got,
        &[
            NAN, -2.0, -2.0, 0.0, 0.0, 7.5, 7.5, 7.5, NAN, NAN, 1.0, 1.0, -1e9, -1e9, NAN,
        ],
        "at STEP",
    );

    let linear = series(&t, &v, Interp::Linear);
    let got: Vec<f64> = probes.iter().map(|&p| linear.at(p)).collect();
    same(
        &got,
        &[
            NAN,
            -2.0,
            -1.333_333_333_333_333_5,
            0.0,
            3.0,
            7.5,
            NAN,
            NAN,
            NAN,
            NAN,
            1.0,
            -5.384_615_38e8,
            -1e9,
            -1e9,
            NAN,
        ],
        "at LINEAR",
    );
}

/// `lower_bound`'s two awkward inputs — a `NaN` probe and a run of equal
/// timestamps — decide which sample `at` reports, so they are pinned against
/// the Java rather than reasoned about.
#[test]
fn lower_bound_matches_the_java_on_duplicates_and_nan() {
    let base = [-3.5, -3.5, -1.0, 0.0, 0.0, 0.0, 2.25, 7.0, 7.0, 19.5];
    let probes = [
        -1e9, -3.5, -3.4999, -1.0, -0.5, -0.0, 0.0, 1e-12, 2.25, 6.99, 7.0, 7.0001, 19.5, 19.6,
        1e9, NAN,
    ];
    let got: Vec<usize> = probes.iter().map(|&p| lower_bound(&base, p)).collect();
    assert_eq!(
        got,
        vec![0, 0, 2, 2, 3, 3, 3, 6, 6, 7, 7, 9, 9, 10, 10, 0],
        "lower_bound"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// EnvelopeDecimator
// ════════════════════════════════════════════════════════════════════════════

/// The representative time is the **midpoint sample's** time, not the mean of
/// the bucket's span — a distinction that is invisible on a uniform master and
/// enormous on the irregular one used here (`t[i] = i²/2`), with gaps at every
/// third sample so the all-`NaN` buckets are exercised too.
#[test]
fn min_max_picks_the_javas_representative_time_at_every_bucket_count() {
    let t: Vec<f64> = (0..10).map(|i| (i * i) as f64 * 0.5).collect();
    let v: Vec<f64> = (0..10)
        .map(|i| if i % 3 == 0 { NAN } else { (i - 5) as f64 })
        .collect();

    /// `(buckets, expected t, expected min, expected max)`.
    type Bucketing<'a> = (usize, &'a [f64], &'a [f64], &'a [f64]);

    let cases: &[Bucketing] = &[
        (1, &[8.0], &[-4.0], &[3.0]),
        (2, &[2.0, 24.5], &[-4.0, 0.0], &[-1.0, 3.0]),
        (3, &[0.5, 8.0, 24.5], &[-4.0, -1.0, 2.0], &[-3.0, 0.0, 3.0]),
        (
            4,
            &[0.0, 4.5, 12.5, 32.0],
            &[-4.0, -3.0, 0.0, 2.0],
            &[-4.0, -1.0, 0.0, 3.0],
        ),
        (
            7,
            &[0.0, 0.5, 2.0, 8.0, 12.5, 24.5, 32.0],
            &[NAN, -4.0, -3.0, -1.0, 0.0, 2.0, 3.0],
            &[NAN, -4.0, -3.0, -1.0, 0.0, 2.0, 3.0],
        ),
        // Asking for at least as many buckets as there are samples collapses to
        // one sample per bucket, and the all-gap buckets report NaN rather than
        // the ±∞ their accumulators still hold.
        (
            10,
            &[0.0, 0.5, 2.0, 4.5, 8.0, 12.5, 18.0, 24.5, 32.0, 40.5],
            &[NAN, -4.0, -3.0, NAN, -1.0, 0.0, NAN, 2.0, 3.0, NAN],
            &[NAN, -4.0, -3.0, NAN, -1.0, 0.0, NAN, 2.0, 3.0, NAN],
        ),
        (
            20,
            &[0.0, 0.5, 2.0, 4.5, 8.0, 12.5, 18.0, 24.5, 32.0, 40.5],
            &[NAN, -4.0, -3.0, NAN, -1.0, 0.0, NAN, 2.0, 3.0, NAN],
            &[NAN, -4.0, -3.0, NAN, -1.0, 0.0, NAN, 2.0, 3.0, NAN],
        ),
    ];
    for &(buckets, want_t, want_min, want_max) in cases {
        let e = min_max(&t, &v, 0, 9, buckets);
        same(&e.t, want_t, &format!("min_max t, buckets={buckets}"));
        same(&e.min, want_min, &format!("min_max min, buckets={buckets}"));
        same(&e.max, want_max, &format!("min_max max, buckets={buckets}"));
    }

    // A sub-range: edges are computed relative to the range, not the array.
    let e = min_max(&t, &v, 2, 7, 3);
    same(&e.t, &[2.0, 8.0, 18.0], "sub-range t");
    same(&e.min, &[-3.0, -1.0, 2.0], "sub-range min");
    same(&e.max, &[-3.0, 0.0, 2.0], "sub-range max");
}

/// Three million samples through the same bucket arithmetic the browser uses.
///
/// This is the shape that motivated widening `edge`'s multiply to `u64`:
/// `bucket * n` leaves `u32` at 70 000 samples, and `usize` is 32-bit on
/// wasm32, so a release build would have wrapped a boundary silently. The host
/// cannot execute 32-bit `usize` arithmetic, so what this actually proves is
/// narrower and still worth having — the widened form agrees with the Java at
/// scale, bucket index for bucket index. The oracle ran the identical three
/// million samples and reported `max = 9999 @ 711`, `min = −9999 @ 888`.
#[test]
fn a_three_million_sample_envelope_agrees_with_the_java() {
    let n = 3_000_000usize;
    let t: Vec<f64> = (0..n).map(|i| i as f64 * 1e-4).collect();
    let mut v: Vec<f64> = (0..n)
        .map(|i| (((i as u64).wrapping_mul(2_654_435_761) >> 7) % 1000) as f64 - 500.0)
        .collect();
    v[1_777_777] = 9999.0;
    v[2_222_222] = -9999.0;

    let e = min_max(&t, &v, 0, n - 1, 1200);
    assert_eq!(e.len(), 1200);
    let (mut hi, mut lo, mut hi_bucket, mut lo_bucket) =
        (f64::NEG_INFINITY, f64::INFINITY, 0usize, 0usize);
    for b in 0..e.len() {
        if e.max[b] > hi {
            hi = e.max[b];
            hi_bucket = b;
        }
        if e.min[b] < lo {
            lo = e.min[b];
            lo_bucket = b;
        }
    }
    assert_eq!((hi, hi_bucket), (9999.0, 711), "the planted spike");
    assert_eq!((lo, lo_bucket), (-9999.0, 888), "the planted dip");
    same(
        &[e.t[0], e.t[599], e.t[1199]],
        &[0.124_900_000_000_000_01, 149.8749, 299.8749],
        "representative times at scale",
    );
}

// ════════════════════════════════════════════════════════════════════════════
// MergedRaster
// ════════════════════════════════════════════════════════════════════════════

/// `union` sorts then drops adjacent duplicates, and `fixed` accumulates as
/// `t0 + i * dt`. Both are pinned on their exact float output, including the
/// entries that are not the decimal they look like.
#[test]
fn union_and_fixed_reproduce_the_javas_exact_floats() {
    let a = [0.0, 2.0, 4.0];
    let b = [1.0, 2.0, 3.0];
    same(
        &union(&[&a[..], &b[..], &[]], 100).unwrap(),
        &[0.0, 1.0, 2.0, 3.0, 4.0],
        "union",
    );
    // An unsorted base is sorted rather than merged in place — the Java sorts
    // the whole concatenation unconditionally, so a corrupt master still
    // dedupes rather than passing duplicates through.
    same(
        &union(&[&[5.0, 1.0, 3.0, 1.0][..]], 100).unwrap(),
        &[1.0, 3.0, 5.0],
        "union of an unsorted base",
    );
    // Signed zero folds and every NaN survives, because dedupe is IEEE `==`.
    let merged = union(&[&[-0.0, 0.0, NAN][..], &[0.0, NAN][..]], 100).unwrap();
    assert_eq!(merged.len(), 3);
    assert!(merged[0] == 0.0 && merged[0].is_sign_negative());
    assert!(merged[1].is_nan() && merged[2].is_nan());

    // 0.7 is not representable, so the accumulation drifts — identically.
    same(
        &fixed(0.0, 10.0, 0.7, 1000).unwrap(),
        &[
            0.0,
            0.7,
            1.4,
            2.099_999_999_999_999_6,
            2.8,
            3.5,
            4.199_999_999_999_999,
            4.899_999_999_999_999_5,
            5.6,
            6.3,
            7.0,
            7.699_999_999_999_999,
            8.399_999_999_999_999,
            9.1,
            9.799_999_999_999_999,
        ],
        "fixed dt = 0.7",
    );
    // A dyadic dt is exact all the way, in both.
    same(
        &fixed(-1.0, 1.0, 0.125, 1000).unwrap(),
        &[
            -1.0, -0.875, -0.75, -0.625, -0.5, -0.375, -0.25, -0.125, 0.0, 0.125, 0.25, 0.375, 0.5,
            0.625, 0.75, 0.875, 1.0,
        ],
        "fixed dt = 0.125",
    );
}

// ════════════════════════════════════════════════════════════════════════════
// The four time operators
// ════════════════════════════════════════════════════════════════════════════

/// The trapezoid, against a hand computation *and* against the Java.
///
/// `t = [0, 0.5, 2.5, 3, 7]`, `v = [2, 4, −1, −1, 3]`, so the four segments are
/// `½(2+4)(0.5) = 1.5`, `½(4−1)(2) = 3`, `½(−1−1)(0.5) = −0.5` and
/// `½(−1+3)(4) = 4` — cumulative `[0, 1.5, 4.5, 4, 8]`, which is what the Java
/// prints. Both agreements matter: the hand computation says the formula is
/// right, the oracle says the *rounding* is.
#[test]
fn the_trapezoid_integral_matches_a_hand_computation_and_the_java() {
    let t = [0.0, 0.5, 2.5, 3.0, 7.0];
    let v = [2.0, 4.0, -1.0, -1.0, 3.0];
    same(
        &run("integral(x)", &t, &one(&t, &v, Interp::Linear)),
        &[0.0, 1.5, 4.5, 4.0, 8.0],
        "integral over an irregular raster",
    );

    // The rule is exact on a straight line: ∫3t dt over [0, 4] is 24, reached
    // at index 400 of a 0.01 s raster.
    let raster: Vec<f64> = (0..1001).map(|i| i as f64 * 0.01).collect();
    let lin: Vec<f64> = raster.iter().map(|t| 3.0 * t).collect();
    let out = run("integral(x)", &raster, &one(&raster, &lin, Interp::Linear));
    assert!((out[400] - 24.0).abs() < 1e-9, "∫3t dt = {}", out[400]);

    // A gap holds the accumulator flat and it resumes afterwards: the area
    // *under* the gap is lost, the area after it is not.
    let t = [0.0, 1.0, 2.0, 3.0, 4.0];
    let v = [1.0, NAN, 3.0, 4.0, 5.0];
    same(
        &run("integral(x)", &t, &one(&t, &v, Interp::Step)),
        &[0.0, 0.0, 0.0, 3.5, 8.0],
        "integral across a gap",
    );
    same(
        &run("delta(x)", &t, &one(&t, &v, Interp::Step)),
        &[0.0, NAN, NAN, 1.0, 1.0],
        "delta across a gap",
    );
    same(
        &run("movavg(x, 2)", &t, &one(&t, &v, Interp::Step)),
        &[1.0, 1.0, 2.0, 3.5, 4.0],
        "movavg across a gap",
    );
}

/// Which side of `t − window` the trailing mean includes.
///
/// The Java's loop is `while (t[start] < t[i] - window)`, so a sample landing
/// **exactly** on `t − window` is kept. The three-way probe is the proof rather
/// than the reasoning: at `t = 1` over `v = [1, 2, …]` a window of exactly 1
/// gives `1.5` (both samples), a window one ULP under 1 gives `2.0` (the
/// boundary sample dropped), and one ULP over changes nothing.
#[test]
fn the_movavg_window_includes_the_sample_on_its_boundary() {
    let t = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
    let v = [1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
    let inputs = one(&t, &v, Interp::Linear);

    same(
        &run("movavg(x, 1)", &t, &inputs),
        &[1.0, 1.5, 3.0, 6.0, 12.0, 24.0],
        "window exactly 1",
    );
    same(
        &run("movavg(x, 0.9999999999999999)", &t, &inputs),
        &[1.0, 2.0, 3.0, 6.0, 12.0, 24.0],
        "window one ULP under 1 drops the boundary sample",
    );
    same(
        &run("movavg(x, 1.0000000000000002)", &t, &inputs),
        &[1.0, 1.5, 3.0, 6.0, 12.0, 24.0],
        "window one ULP over 1",
    );
    same(
        &run("movavg(x, 2)", &t, &inputs),
        &[
            1.0,
            1.5,
            2.333_333_333_333_333_5,
            4.666_666_666_666_667,
            9.333_333_333_333_334,
            18.666_666_666_666_668,
        ],
        "window 2",
    );
}

/// `delay` reads the *source* series at `t − τ` through the source's own
/// interpolation mode, not a shifted copy of the rastered column — so on a
/// raster finer than the source the two modes give visibly different answers,
/// and both are pinned.
#[test]
fn delay_reads_the_source_series_in_its_own_mode() {
    let src = [0.0, 1.0, 2.0, 3.0, 4.0];
    let sv = [0.0, 1.0, 4.0, 9.0, 16.0];
    let raster = [-0.5, 0.0, 0.25, 1.0, 1.5, 2.0, 3.999, 4.0, 4.5];
    let step = one(&src, &sv, Interp::Step);
    let linear = one(&src, &sv, Interp::Linear);

    let cases: &[(&str, Interp, [f64; 9])] = &[
        (
            "delay(x, 0.5)",
            Interp::Step,
            [NAN, NAN, NAN, 0.0, 1.0, 1.0, 9.0, 9.0, 16.0],
        ),
        (
            "delay(x, 0)",
            Interp::Step,
            [NAN, 0.0, 0.0, 1.0, 1.0, 4.0, 9.0, 16.0, 16.0],
        ),
        (
            "delay(x, 2)",
            Interp::Step,
            [NAN, NAN, NAN, NAN, NAN, 0.0, 1.0, 4.0, 4.0],
        ),
        (
            "delay(x, 0.5)",
            Interp::Linear,
            [NAN, NAN, NAN, 0.5, 1.0, 2.5, 12.493, 12.5, 16.0],
        ),
        (
            "delay(x, 0)",
            Interp::Linear,
            [NAN, 0.0, 0.25, 1.0, 2.5, 4.0, 15.993, 16.0, 16.0],
        ),
        (
            "delay(x, 2)",
            Interp::Linear,
            [
                NAN,
                NAN,
                NAN,
                NAN,
                NAN,
                0.0,
                3.997_000_000_000_000_3,
                4.0,
                6.5,
            ],
        ),
    ];
    for (formula, interp, want) in cases {
        let inputs = if *interp == Interp::Step {
            &step
        } else {
            &linear
        };
        same(
            &run(formula, &raster, inputs),
            want,
            &format!("{formula} {interp:?}"),
        );
    }
}

/// A stalled time master (duplicated timestamps) and a corrupt one (a `NaN`
/// timestamp) are both things a real recording does, and both change every
/// downstream operator.
///
/// The `NaN` case is the reason this file quotes the oracle instead of the
/// source: a `NaN` in the master breaks `lower_bound`'s *search*, not its
/// arithmetic — `t[mid] < x` is false at the gap, so the bisection turns left
/// and never sees the samples beyond it. `x` therefore reads `[1, 2, NaN, 2, 2]`
/// rather than the `[1, 2, NaN, 4, 5]` the code appears to say, and the
/// trapezoid consequently *survives* (the `NaN` value skips both segments that
/// would have used the `NaN` time) rather than being poisoned. Java does
/// exactly this, defect for defect.
#[test]
fn a_stalled_or_corrupt_time_master_gives_the_javas_answers() {
    // t[3] == t[4]: `lower_bound` resolves a run to its first index, so the
    // sampled column reads 4.0 twice and the fourth trapezoid segment is empty.
    let ti = [0.0, 0.1, 0.15, 5.0, 5.0, 9.0];
    let vi = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let inputs = one(&ti, &vi, Interp::Step);
    same(
        &run("x", &ti, &inputs),
        &[1.0, 2.0, 3.0, 4.0, 4.0, 6.0],
        "x",
    );
    same(
        &run("delta(x)", &ti, &inputs),
        &[0.0, 1.0, 1.0, 1.0, 0.0, 2.0],
        "delta",
    );
    same(
        &run("integral(x)", &ti, &inputs),
        &[
            0.0,
            0.150_000_000_000_000_02,
            0.275,
            17.249_999_999_999_996,
            17.249_999_999_999_996,
            37.25,
        ],
        "integral",
    );
    same(
        &run("movavg(x, 5)", &ti, &inputs),
        &[1.0, 1.5, 2.0, 2.5, 2.8, 4.666_666_666_666_667],
        "movavg",
    );

    let tn = [0.0, 1.0, NAN, 3.0, 4.0];
    let vn = [1.0, 2.0, 3.0, 4.0, 5.0];
    let inputs = one(&tn, &vn, Interp::Step);
    same(&run("x", &tn, &inputs), &[1.0, 2.0, NAN, 2.0, 2.0], "x");
    same(
        &run("delta(x)", &tn, &inputs),
        &[0.0, 1.0, NAN, NAN, 0.0],
        "delta",
    );
    same(
        &run("integral(x)", &tn, &inputs),
        &[0.0, 1.5, 1.5, 1.5, 3.5],
        "integral",
    );
    same(
        &run("movavg(x, 2)", &tn, &inputs),
        &[1.0, 1.5, 1.5, 2.0, 2.0],
        "movavg",
    );
    same(
        &run("delay(x, 1)", &tn, &inputs),
        &[NAN, 1.0, NAN, 2.0, 2.0],
        "delay",
    );
}

/// Infinities are values, not gaps: they are summed, differenced and averaged
/// like anything else, which is how `∞ − ∞` becomes a `NaN` the user has to
/// interpret. Pinned because the tempting "sanitise non-finites" change would
/// silently alter every one of these — and because the calc path's division is
/// bare IEEE, so a stopped sensor manufactures an `∞` on its own.
#[test]
fn infinities_flow_through_the_operators_as_the_java_lets_them() {
    let t = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
    let v = [INF, 1.0, -INF, 2.0, 3.0, 4.0];
    let inputs = one(&t, &v, Interp::Step);
    same(
        &run("delta(x)", &t, &inputs),
        &[0.0, -INF, -INF, INF, 1.0, 1.0],
        "delta",
    );
    same(
        &run("integral(x)", &t, &inputs),
        &[0.0, INF, NAN, NAN, NAN, NAN],
        "integral",
    );
    // **Deliberate divergence, and the one place this file records the port
    // giving a different answer from the oracle on purpose.**
    //
    // Java: `[∞, ∞, NaN, NaN, NaN, NaN]`. Its running sum is poisoned for good
    // once ∞ and −∞ have both been inside the window, because the subtraction
    // that drops them out is `NaN - x` — a one-way door. The first three entries
    // are genuine: at t = 2 the window really does hold ∞ and −∞ at once, and
    // `∞ + (−∞)` is `NaN`.
    //
    // The last three are not. At t = 5 the trailing 2 s window holds `{2, 3, 4}`
    // and nothing else — the infinities left two points ago — and the mean of
    // those is 3. Java reports a gap there, and would keep reporting one for
    // every remaining point of a 500 000-sample channel. Manufacturing gaps out
    // of good data is the same rule `series.rs` states in the other direction
    // ("gaps stay gaps"), broken; a wrong answer that looks like missing data is
    // exactly what this port refuses to inherit, on the same grounds as
    // `raster::fixed` over an infinite span.
    //
    // So `movavg` recomputes the window sum when the accumulator stops being
    // finite, and the entries below are the *correct* trailing means. The branch
    // is unreachable for a channel whose sums stay finite, so every ordinary
    // recording still matches the Java bit for bit — see `calc.rs::movavg` for
    // the bound on the repair and for the cancellation case it cannot fix.
    same(
        &run("movavg(x, 2)", &t, &inputs),
        &[INF, INF, NAN, -INF, -INF, 3.0],
        "movavg",
    );
}

/// The same `^` rule, at the **second** site the port evaluates `^` from.
///
/// [`a_gap_in_the_exponent_stays_a_gap`] fixed `Math.pow` in the compiled calc
/// tree. But a function call is *not* compiled — the whole subtree is handed to
/// the document evaluator ([`frees_core::eval`]), which is the entire reason the
/// fallback exists (it is how a formula reaches the CoolProp property library).
/// That evaluator had C's `pow`, so wrapping the identical expression in any
/// call put the invented `1.0` straight back:
///
/// ```text
///                          before          Java (oracle)
/// abs(high ^ ratio)   [1.0, 1.0, 1.0]   [1.0, NaN, 1.0]
/// abs(high ^ inf)     [1.0, 1.0, 1.0]   [NaN, NaN, NaN]
/// ln(high ^ ratio)    [0.0, 0.0, 0.0]   [0.0, NaN, 0.0]
/// ```
///
/// `abs(high ^ inf)` is the sharp one: every sample was wrong, not just the
/// gap. Java's `ast/Evaluator` uses `Math.pow` exactly as `TimeSeriesEvaluator`
/// does, so this was a defect at both sites and is now fixed at both.
#[test]
fn a_gap_in_the_exponent_stays_a_gap_inside_a_call_argument() {
    let t = [0.0, 1.0, 2.0];
    let inputs = bind(&[
        ("high", series(&t, &[1.0, 1.0, 1.0], Interp::Step)),
        ("ratio", series(&t, &[2.0, NAN, 0.5], Interp::Step)),
        ("inf", series(&t, &[INF, INF, INF], Interp::Step)),
    ]);
    for f in [
        "abs(high ^ ratio)",
        "abs(0 - (high ^ ratio))",
        "max(high ^ ratio, 0)",
        "sqrt(high ^ ratio)",
    ] {
        same(&run(f, &t, &inputs), &[1.0, NAN, 1.0], f);
    }
    same(
        &run("ln(high ^ ratio)", &t, &inputs),
        &[0.0, NAN, 0.0],
        "ln",
    );
    same(
        &run("abs(high ^ inf)", &t, &inputs),
        &[NAN, NAN, NAN],
        "abs(high ^ inf)",
    );
}

// ════════════════════════════════════════════════════════════════════════════
// Known, unfixed divergences
// ════════════════════════════════════════════════════════════════════════════

/// Inside a call argument the calc path uses the *document* evaluator, whose
/// division-by-zero guard the Java's `ast/Evaluator` does not have.
///
/// The Java answers `abs(1 / 0)` with `+∞`; here it is a typed failure that
/// takes the whole channel. The guard exists because a residual that silently
/// became `∞` would poison a Newton block, it is shared with every solve in the
/// engine, and it is out of this module's reach — so this test is a *record*,
/// not an aspiration. It is also why the short-circuit fix matters more here
/// than it did in Java: guarding the call is the only workaround available.
#[test]
fn division_by_zero_inside_a_call_still_diverges_from_the_java() {
    let raster = [0.0, 1.0, 2.0];
    let inputs = one(&raster, &[0.0, 2.0, 0.0], Interp::Step);

    // Java: [1.0, 0.0, 1.0]. Here: a failure at the first zero sample.
    let m = run_err("x < 1 and abs(1 / x) > 0", &raster, &inputs);
    assert!(m.contains("division by zero"), "{m}");
    assert!(m.contains("t = 0"), "{m}");

    // Outside a call the bare operator *is* IEEE, in both — so the divergence
    // is exactly the call boundary and nothing wider.
    same(
        &run("1 / x", &raster, &inputs),
        &[INF, 0.5, INF],
        "bare division",
    );
    // And the workaround the fix above enables.
    same(
        &run("x > 1 and abs(1 / x) > 0", &raster, &inputs),
        &[0.0, 1.0, 0.0],
        "guarded, as the Java answers it",
    );
}

/// `^`'s *other* two document-evaluator guards, which are the same shape as the
/// division one above and are pinned for the same reason.
///
/// `crate::eval` refuses a negative base with a non-integer exponent and a zero
/// base with a negative one; Java's `ast/Evaluator` answers `NaN` and `+∞`. As
/// with division the divergence is exactly the call boundary — the bare
/// operator is `Math.pow`-faithful on both sides — but it is a *louder* failure
/// than division's, because the "non-integer exponent" test is `r != floor(r)`
/// and `NaN != NaN`: one dropout in an exponent channel over a negative base
/// fails the entire signal where the Java loses only that sample.
///
/// Left in place because the guards are engine-wide and load-bearing for
/// Newton, exactly like the division one. Recorded so the boundary of what the
/// `java_pow` work fixed is written down rather than assumed.
#[test]
fn the_document_evaluators_power_guards_still_diverge_from_the_java() {
    let t = [0.0, 1.0, 2.0];
    let inputs = bind(&[
        ("negb", series(&t, &[-8.0, -8.0, -8.0], Interp::Step)),
        ("half", series(&t, &[0.5, 0.5, 0.5], Interp::Step)),
        ("zero", series(&t, &[0.0, 0.0, 0.0], Interp::Step)),
        ("negone", series(&t, &[-1.0, -1.0, -1.0], Interp::Step)),
        ("gap", series(&t, &[1.0, NAN, 1.0], Interp::Step)),
    ]);

    // Java: [NaN, NaN, NaN]. Here: refused.
    let m = run_err("abs(negb ^ half)", &t, &inputs);
    assert!(m.contains("negative base"), "{m}");
    // Java: [inf, inf, inf]. Here: refused.
    let m = run_err("abs(zero ^ (0 - 1))", &t, &inputs);
    assert!(m.contains("division by zero"), "{m}");
    // Java: [1.0, NaN, 1.0] — only the gap sample is lost. Here: the whole
    // channel, and the report names the gap's timestamp.
    let m = run_err("abs(negone ^ gap)", &t, &inputs);
    assert!(m.contains("negative base"), "{m}");
    assert!(m.contains("t = 1"), "{m}");

    // Outside a call all three are the Java's own answers, so the divergence is
    // the call boundary and nothing wider.
    same(&run("negb ^ half", &t, &inputs), &[NAN, NAN, NAN], "bare");
    same(
        &run("zero ^ (0 - 1)", &t, &inputs),
        &[INF, INF, INF],
        "bare",
    );
    same(
        &run("negb ^ gap", &t, &inputs),
        &[-8.0, NAN, -8.0],
        "bare, gap in the exponent",
    );
}
