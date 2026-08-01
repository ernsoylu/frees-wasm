//! One calculated-signal input: samples plus how to read between them.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/measurement/SampledSeries.java`
//! (71 LOC).
//!
//! The interpolation mode is a per-input choice, not a global setting, and the
//! Java's reasoning for that is worth keeping: [`Interp::Step`] (hold the last
//! sample) is the honest reading of a boolean, an enum or an ECU state, where
//! a value between two samples never existed; [`Interp::Linear`] is the honest
//! reading of a continuous analog channel feeding a nonlinear property
//! function, because step-holding `T`/`P` into a property call manufactures
//! derivative spikes that are artefacts of the sampling, not of the physics.
//!
//! The one rule that outranks both: **a gap is never bridged.** `NaN` marks
//! "no measurement here", so an interpolation with a `NaN` at either end is
//! `NaN`, and a query before the first sample is `NaN` rather than the first
//! value — there is nothing behind it to hold.

use crate::measurement::decimate::lower_bound;

/// How to evaluate the series between two samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interp {
    /// Hold the last sample at or before the query.
    Step,
    /// Straight line between the bracketing samples.
    Linear,
}

impl Interp {
    /// The wire spelling the frontend already sends and parses
    /// (`analyzer/measurementApi.ts`: `interp: 'step' | 'linear'`).
    pub fn as_str(self) -> &'static str {
        match self {
            Interp::Step => "step",
            Interp::Linear => "linear",
        }
    }
}

/// A time-ordered sample set with an interpolation mode.
///
/// `t` must be ascending — [`at`](SampledSeries::at) binary-searches it. `t`
/// and `v` are parallel; if they differ in length the series behaves as its
/// common prefix (see [`at`](SampledSeries::at)).
#[derive(Debug, Clone, PartialEq)]
pub struct SampledSeries {
    pub t: Vec<f64>,
    pub v: Vec<f64>,
    pub interp: Interp,
}

impl SampledSeries {
    pub fn new(t: Vec<f64>, v: Vec<f64>, interp: Interp) -> Self {
        SampledSeries { t, v, interp }
    }

    /// Value at time `x`.
    ///
    /// Exact port of the Java `at`, whose five rules are all load-bearing:
    ///
    /// * an empty series is `NaN` everywhere;
    /// * an exact hit on a sample time returns **that sample**, whatever the
    ///   interpolation mode, and whatever its value — including `NaN`;
    /// * `x` before the first sample is `NaN`, not the first value: there is
    ///   nothing behind it to hold, and reporting one would invent data at the
    ///   start of every calculated signal;
    /// * `x` past the last sample, or [`Interp::Step`], returns the last
    ///   sample at or before `x`;
    /// * otherwise the linear blend of the bracketing samples — **unless
    ///   either of them is `NaN`, in which case the answer is `NaN`.** Gaps
    ///   stay gaps; interpolation must not bridge one.
    ///
    /// **Divergence from the Java.** With a `v` shorter than `t` the Java
    /// throws `ArrayIndexOutOfBoundsException`; here the series is treated as
    /// its common prefix. Mismatched lengths are a caller bug either way, but
    /// under `panic = "abort"` in wasm the throw takes the whole module — and
    /// with it the file the user was promised would never leave the tab.
    pub fn at(&self, x: f64) -> f64 {
        let n = self.t.len().min(self.v.len());
        if n == 0 {
            return f64::NAN;
        }
        let t = &self.t[..n];
        let v = &self.v[..n];

        let lb = lower_bound(t, x);
        if lb < n && t[lb] == x {
            return v[lb];
        }
        if lb == 0 {
            // Java's `i = lb - 1; if (i < 0) return NaN` — before the first
            // sample, and also the whole answer for `x` = NaN, which sends
            // `lower_bound` to the left edge.
            return f64::NAN;
        }
        let i = lb - 1; // last sample strictly before x
        if self.interp == Interp::Step || lb >= n {
            return v[i];
        }

        let t0 = t[i];
        let t1 = t[lb];
        let v0 = v[i];
        let v1 = v[lb];
        if v0.is_nan() || v1.is_nan() {
            return f64::NAN;
        }
        v0 + (v1 - v0) * (x - t0) / (t1 - t0)
    }

    /// Materialise this series on an output raster.
    pub fn sample_on(&self, raster: &[f64]) -> Vec<f64> {
        raster.iter().map(|&x| self.at(x)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(t: &[f64], v: &[f64]) -> SampledSeries {
        SampledSeries::new(t.to_vec(), v.to_vec(), Interp::Step)
    }

    fn linear(t: &[f64], v: &[f64]) -> SampledSeries {
        SampledSeries::new(t.to_vec(), v.to_vec(), Interp::Linear)
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-12, "{a} != {b}");
    }

    #[test]
    fn an_empty_series_is_nan_everywhere() {
        for interp in [Interp::Step, Interp::Linear] {
            let s = SampledSeries::new(vec![], vec![], interp);
            assert!(s.at(0.0).is_nan());
            assert!(s.at(-1e9).is_nan());
            assert!(s.at(1e9).is_nan());
            assert!(s.at(f64::NAN).is_nan());
        }
    }

    #[test]
    fn an_exact_hit_returns_that_sample() {
        let t = [0.0, 1.0, 2.0];
        let v = [10.0, 20.0, 40.0];
        for s in [step(&t, &v), linear(&t, &v)] {
            close(s.at(0.0), 10.0);
            close(s.at(1.0), 20.0);
            close(s.at(2.0), 40.0);
        }
    }

    #[test]
    fn an_exact_hit_on_a_gap_returns_the_gap() {
        // The sample *is* NaN; there is nothing to interpolate around.
        let s = linear(&[0.0, 1.0, 2.0], &[10.0, f64::NAN, 40.0]);
        assert!(s.at(1.0).is_nan());
    }

    #[test]
    fn before_the_first_sample_is_nan_not_the_first_value() {
        let t = [1.0, 2.0];
        let v = [10.0, 20.0];
        for s in [step(&t, &v), linear(&t, &v)] {
            assert!(s.at(0.999_999).is_nan());
            assert!(s.at(0.0).is_nan());
            assert!(s.at(-1e12).is_nan());
            assert!(s.at(f64::NEG_INFINITY).is_nan());
        }
    }

    #[test]
    fn past_the_last_sample_holds_the_last_value() {
        // Even under Linear: there is no second point to blend towards.
        let t = [0.0, 1.0, 2.0];
        let v = [10.0, 20.0, 40.0];
        for s in [step(&t, &v), linear(&t, &v)] {
            close(s.at(2.000_001), 40.0);
            close(s.at(1e9), 40.0);
            close(s.at(f64::INFINITY), 40.0);
        }
    }

    #[test]
    fn step_holds_the_last_sample_at_or_before_x() {
        let s = step(&[0.0, 1.0, 2.0], &[10.0, 20.0, 40.0]);
        close(s.at(0.001), 10.0);
        close(s.at(0.999), 10.0);
        close(s.at(1.5), 20.0);
        close(s.at(1.999_999), 20.0);
    }

    #[test]
    fn linear_blends_the_bracketing_samples() {
        let s = linear(&[0.0, 1.0, 2.0], &[10.0, 20.0, 40.0]);
        close(s.at(0.5), 15.0);
        close(s.at(0.25), 12.5);
        close(s.at(1.5), 30.0);
        close(s.at(1.75), 35.0);
    }

    #[test]
    fn linear_is_exact_on_a_straight_line_with_irregular_spacing() {
        let t = [0.0, 0.1, 5.0, 5.001, 12.0];
        let f = |x: f64| 3.0 * x - 7.0;
        let v: Vec<f64> = t.iter().map(|&x| f(x)).collect();
        let s = SampledSeries::new(t.to_vec(), v, Interp::Linear);
        for probe in [0.05, 1.0, 4.999, 5.000_5, 6.0, 11.999] {
            close(s.at(probe), f(probe));
        }
    }

    #[test]
    fn linear_never_bridges_a_gap() {
        // The Java's own comment: "gaps stay gaps; never bridged by
        // interpolation". A NaN at either end poisons the whole interval.
        let s = linear(&[0.0, 1.0, 2.0], &[10.0, f64::NAN, 40.0]);
        assert!(s.at(0.5).is_nan(), "right end NaN");
        assert!(s.at(1.5).is_nan(), "left end NaN");
        // The intervals that touch no gap are unaffected.
        let s = linear(&[0.0, 1.0, 2.0, 3.0], &[10.0, f64::NAN, 40.0, 60.0]);
        close(s.at(2.5), 50.0);
    }

    #[test]
    fn step_reports_a_gap_it_is_holding() {
        // Not an interpolation rule — the held sample simply is NaN.
        let s = step(&[0.0, 1.0, 2.0], &[10.0, f64::NAN, 40.0]);
        assert!(s.at(1.5).is_nan());
        close(s.at(0.5), 10.0);
    }

    #[test]
    fn a_single_sample_series_holds_forwards_only() {
        let t = [5.0];
        let v = [42.0];
        for s in [step(&t, &v), linear(&t, &v)] {
            assert!(s.at(4.999).is_nan());
            close(s.at(5.0), 42.0);
            close(s.at(5.001), 42.0);
            close(s.at(1e6), 42.0);
        }
    }

    #[test]
    fn a_nan_query_is_nan() {
        // `lower_bound(t, NaN)` is 0 (nothing compares below NaN), so `at`
        // takes the before-the-start branch.
        let t = [0.0, 1.0, 2.0];
        let v = [10.0, 20.0, 40.0];
        for s in [step(&t, &v), linear(&t, &v)] {
            assert!(s.at(f64::NAN).is_nan());
        }
    }

    #[test]
    fn duplicate_timestamps_resolve_to_the_first_of_the_run() {
        // A stalled time master. `lower_bound` returns the first index of the
        // run, so `at` reports the earlier of the two values.
        let t = [0.0, 1.0, 1.0, 2.0];
        let v = [10.0, 20.0, 30.0, 40.0];
        let s = linear(&t, &v);
        close(s.at(1.0), 20.0);
        // 1.0 -> 2.0 blends from v[2] = 30 (the last sample strictly before
        // 1.5 is index 2), not from v[1].
        close(s.at(1.5), 35.0);
    }

    #[test]
    fn a_zero_width_interval_cannot_reach_the_division() {
        // Two samples at the same time are the only way to get a zero
        // denominator, and the blend is unreachable for them: the exact-hit
        // branch fires first, and otherwise `t[i] < x < t[lb]` holds by
        // construction of `lower_bound`, so the denominator is strictly
        // positive on every ascending series.
        let s = linear(&[0.0, 1.0, 1.0, 2.0], &[10.0, 20.0, 30.0, 40.0]);
        close(s.at(1.0), 20.0);
        assert!(s.at(0.5).is_finite());
        assert!(s.at(1.5).is_finite());
    }

    #[test]
    fn sample_on_maps_the_whole_raster() {
        let s = linear(&[0.0, 1.0, 2.0], &[10.0, 20.0, 40.0]);
        let out = s.sample_on(&[-1.0, 0.0, 0.5, 1.0, 1.5, 2.0, 3.0]);
        assert_eq!(out.len(), 7);
        assert!(out[0].is_nan());
        close(out[1], 10.0);
        close(out[2], 15.0);
        close(out[3], 20.0);
        close(out[4], 30.0);
        close(out[5], 40.0);
        close(out[6], 40.0);
    }

    #[test]
    fn sample_on_an_empty_raster_is_empty() {
        let s = linear(&[0.0, 1.0], &[1.0, 2.0]);
        assert!(s.sample_on(&[]).is_empty());
        // And an empty series on a real raster is all gaps, not a panic.
        let empty = SampledSeries::new(vec![], vec![], Interp::Linear);
        let out = empty.sample_on(&[0.0, 1.0, 2.0]);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|x| x.is_nan()));
    }

    #[test]
    fn a_short_value_array_truncates_instead_of_panicking() {
        // Divergence from the Java, which throws. See `at`.
        let s = SampledSeries::new(vec![0.0, 1.0, 2.0], vec![10.0, 20.0], Interp::Linear);
        close(s.at(0.5), 15.0);
        close(s.at(1.0), 20.0);
        // Past the end of the *values*, so the last usable sample is held.
        close(s.at(1.5), 20.0);
        close(s.at(2.0), 20.0);
    }

    #[test]
    fn interp_wire_spelling_matches_the_frontend() {
        assert_eq!(Interp::Step.as_str(), "step");
        assert_eq!(Interp::Linear.as_str(), "linear");
    }
}
