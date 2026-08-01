//! Index-uniform min/max envelope decimation.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/measurement/EnvelopeDecimator.java`
//! (94 LOC), which is itself the Java twin of the frontend's
//! `frontend/src/analyzer/decimate.ts`. All three must agree bucket for
//! bucket, because a measurement can be rendered from either side of the wire
//! and a plot that changes shape when the data source changes is a bug report.
//!
//! Two design choices carry over verbatim, and both are about *not lying to
//! the eye*:
//!
//! * **Buckets are index-uniform, not time-uniform.** Equal sample count per
//!   bucket is robust to irregular sampling (an MDF4 group can stall for
//!   seconds) and can never produce an empty bucket.
//! * **The reduction is min/max, not decimate-by-N.** For a boolean or enum
//!   channel the min/max pair *is* the transition flag: a bucket containing a
//!   one-sample pulse spans both levels, so the pulse renders full height
//!   instead of vanishing sub-pixel at some zoom level.
//!
//! `NaN` is a gap, never a value: it is skipped by the reduction, and a bucket
//! with nothing but gaps reports `NaN`/`NaN` rather than the `±∞` its
//! accumulators started at.

/// A per-bucket min/max envelope. The three vectors are aligned by bucket and
/// always the same length.
///
/// The Java `EnvelopeDecimator.Envelope` record, whose hand-written `equals`
/// compared array *contents* — which is what the derived `PartialEq` already
/// gives.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Envelope {
    /// Representative time of each bucket: the time of its **midpoint sample**.
    pub t: Vec<f64>,
    pub min: Vec<f64>,
    pub max: Vec<f64>,
}

impl Envelope {
    /// Number of buckets.
    pub fn len(&self) -> usize {
        self.t.len()
    }

    pub fn is_empty(&self) -> bool {
        self.t.is_empty()
    }
}

/// First index `i` in `[0, a.len())` with `a[i] >= x`, for ascending `a`;
/// `a.len()` when every sample is below `x`.
///
/// Written out rather than delegated to `slice::partition_point` on purpose.
/// The predicate here is *not* guaranteed partitioned — a time master with a
/// `NaN` in it makes `a[mid] < x` false everywhere, including below the gap —
/// and `partition_point` leaves the result unspecified in that case, whereas
/// this loop is deterministic and takes exactly the branches the Java and the
/// TypeScript twin take. Same reason the midpoint is spelled
/// `lo + (hi - lo) / 2`: Java's `(lo + hi) >>> 1` is overflow-safe by virtue
/// of the unsigned shift, and this is the `usize` way to say the same thing.
pub fn lower_bound(a: &[f64], x: f64) -> usize {
    let mut lo = 0usize;
    let mut hi = a.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if a[mid] < x {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Bucket `b`'s start offset within an `n`-sample range split into `m`
/// buckets, i.e. `floor(b * n / m)`.
///
/// The multiply is done in `u64` because **`usize` is 32 bits on wasm32**:
/// `b * n` overflows a `u32` once `n` passes ~65 k samples, which for a
/// measurement window is the ordinary case, not the extreme one. That
/// overflow would not panic in a release build — it would silently scramble
/// the bucket boundaries. The Java widens to `long` at this exact spot for the
/// same reason (its `int` is always 32 bits).
fn edge(b: usize, n: usize, m: usize) -> usize {
    (b as u64 * n as u64 / m as u64) as usize
}

/// Min/max envelope of `v` over the inclusive index range `[i0, i1]`, reduced
/// to at most `buckets` buckets.
///
/// `NaN` samples are skipped; an all-`NaN` bucket yields `NaN`/`NaN`, which
/// renders as a gap. Every bucket is non-empty (`m <= n`), so the returned
/// vectors have exactly `min(max(buckets, 1), n)` entries.
///
/// **Divergence from the Java.** The Java indexes `t`/`v` unchecked and throws
/// `ArrayIndexOutOfBoundsException` on an empty or inverted range (`i1 < i0`
/// makes its midpoint index `(-1) >>> 1`, i.e. `2^31 - 1`) and on any `i1`
/// past the end of the arrays. Here the range is clamped into the data and a
/// degenerate range returns an empty `Envelope`. An out-of-range window is a
/// caller bug either way, but in wasm the panic path is `panic = "abort"` —
/// the whole module dies and the tab loses the file it was promised would
/// never leave it. Returning nothing to plot is the cheaper failure.
///
/// A bucket whose samples are all `+∞` is reported as a gap, because the
/// "nothing seen" sentinel is `+∞` itself. The Java and the TypeScript twin
/// share that blind spot; it is preserved rather than fixed so the three stay
/// bucket-for-bucket identical.
pub fn min_max(t: &[f64], v: &[f64], i0: usize, i1: usize, buckets: usize) -> Envelope {
    // A short `v` truncates the series rather than aborting; see above.
    let Some(last) = t.len().min(v.len()).checked_sub(1) else {
        return Envelope::default();
    };
    let hi = i1.min(last);
    if i0 > hi {
        return Envelope::default();
    }

    let n = hi - i0 + 1;
    // `Math.max(1, Math.min(buckets, n))`. n >= 1 here, so the bounds are
    // well-ordered and `clamp` cannot panic.
    let m = buckets.clamp(1, n);

    let mut out_t = Vec::with_capacity(m);
    let mut out_min = Vec::with_capacity(m);
    let mut out_max = Vec::with_capacity(m);
    for b in 0..m {
        let start = i0 + edge(b, n, m);
        // Inclusive, and >= start: consecutive edges differ by at least
        // n / m >= 1, so no bucket is ever empty.
        let end = i0 + edge(b + 1, n, m) - 1;

        let mut mn = f64::INFINITY;
        let mut mx = f64::NEG_INFINITY;
        for &x in &v[start..=end] {
            if x.is_nan() {
                continue;
            }
            // Explicit comparisons, not `f64::min`/`max`: those pick the
            // non-NaN operand and are unspecified on ±0.0, where the Java's
            // `<`/`>` are exact IEEE.
            if x < mn {
                mn = x;
            }
            if x > mx {
                mx = x;
            }
        }

        // Midpoint *index*, not mean time — the bucket's representative point
        // is a sample that actually exists, which keeps the envelope honest
        // when sampling is irregular.
        out_t.push(t[start + (end - start) / 2]);
        if mn == f64::INFINITY {
            out_min.push(f64::NAN);
            out_max.push(f64::NAN);
        } else {
            out_min.push(mn);
            out_max.push(mx);
        }
    }

    Envelope {
        t: out_t,
        min: out_min,
        max: out_max,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(n: usize, dt: f64) -> Vec<f64> {
        (0..n).map(|i| i as f64 * dt).collect()
    }

    /// Reference implementation of `lower_bound`, by definition.
    fn scan(a: &[f64], x: f64) -> usize {
        a.iter().position(|&y| y >= x).unwrap_or(a.len())
    }

    // ---------------------------------------------------------------- bounds

    #[test]
    fn lower_bound_finds_first_at_least() {
        // The four assertions of the Java EnvelopeDecimatorTest.
        let a = [0.0, 1.0, 2.0, 3.0, 4.0];
        assert_eq!(lower_bound(&a, -1.0), 0);
        assert_eq!(lower_bound(&a, 2.0), 2);
        assert_eq!(lower_bound(&a, 2.5), 3);
        assert_eq!(lower_bound(&a, 99.0), 5);
    }

    #[test]
    fn lower_bound_on_an_empty_slice_is_zero() {
        assert_eq!(lower_bound(&[], 0.0), 0);
        assert_eq!(lower_bound(&[], f64::NAN), 0);
    }

    #[test]
    fn lower_bound_on_duplicates_returns_the_first() {
        let a = [0.0, 1.0, 1.0, 1.0, 2.0];
        assert_eq!(lower_bound(&a, 1.0), 1);
        assert_eq!(lower_bound(&a, 1.5), 4);
    }

    #[test]
    fn lower_bound_of_nan_is_zero() {
        // Every `a[mid] < NaN` is false, so the search collapses to the left
        // edge. Deterministic, and the same answer the Java gives.
        let a = [0.0, 1.0, 2.0, 3.0, 4.0];
        assert_eq!(lower_bound(&a, f64::NAN), 0);
    }

    #[test]
    fn lower_bound_agrees_with_a_linear_scan() {
        // Deterministic LCG, so a failure is reproducible without a seed file.
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as f64 / (1u64 << 31) as f64
        };
        let mut a = Vec::with_capacity(400);
        let mut x = -13.75;
        for _ in 0..400 {
            a.push(x);
            // Non-uniform, and sometimes repeated (a stalled time master).
            let step = next();
            x += if step < 0.1 { 0.0 } else { step };
        }

        for i in 0..a.len() {
            // Exact hits, and probes either side of every sample.
            for probe in [a[i], a[i] - 1e-9, a[i] + 1e-9] {
                assert_eq!(lower_bound(&a, probe), scan(&a, probe), "probe {probe}");
            }
        }
        for probe in [-1e9, -13.75, 0.0, 1e9] {
            assert_eq!(lower_bound(&a, probe), scan(&a, probe), "probe {probe}");
        }
    }

    // ------------------------------------------------------------- envelopes

    #[test]
    fn preserves_spikes_in_a_million_points() {
        // The Java EnvelopeDecimatorTest case, value for value.
        let n = 1_000_000usize;
        let t = ramp(n, 0.001);
        let mut v: Vec<f64> = (0..n).map(|i| (i as f64 / 1000.0).sin()).collect();
        v[733_211] = 42.5;
        v[211_733] = -37.25;

        let env = min_max(&t, &v, 0, n - 1, 1200);
        assert_eq!(env.len(), 1200);
        let hi = env.max.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let lo = env.min.iter().copied().fold(f64::INFINITY, f64::min);
        assert!((hi - 42.5).abs() < 1e-12, "{hi}");
        assert!((lo + 37.25).abs() < 1e-12, "{lo}");
    }

    #[test]
    fn never_loses_a_boolean_pulse() {
        // One 1-sample pulse in a million, decimated 2000:1, must still reach
        // full height in exactly one bucket.
        let n = 1_000_000usize;
        let t = ramp(n, 0.001);
        let mut v = vec![0.0f64; n];
        v[481_997] = 1.0;

        let env = min_max(&t, &v, 0, n - 1, 500);
        let mut pulse_buckets = 0;
        for b in 0..env.len() {
            if env.max[b] == 1.0 {
                pulse_buckets += 1;
                assert_eq!(env.min[b], 0.0);
            }
        }
        assert_eq!(pulse_buckets, 1);
    }

    #[test]
    fn nan_gaps_stay_gaps() {
        let n = 1000usize;
        let t = ramp(n, 0.001);
        let v: Vec<f64> = (0..n)
            .map(|i| if i < 500 { f64::NAN } else { 1.0 })
            .collect();
        let env = min_max(&t, &v, 0, n - 1, 10);
        assert!(env.min[0].is_nan());
        assert!(env.max[0].is_nan());
        assert_eq!(env.min[9], 1.0);
    }

    #[test]
    fn an_all_nan_bucket_is_nan_not_infinity() {
        // The accumulators start at ±∞; leaking them would plot a gap as a
        // full-scale spike.
        let t = ramp(4, 1.0);
        let v = [f64::NAN, f64::NAN, f64::NAN, f64::NAN];
        let env = min_max(&t, &v, 0, 3, 2);
        assert_eq!(env.len(), 2);
        assert!(env.min.iter().all(|x| x.is_nan()));
        assert!(env.max.iter().all(|x| x.is_nan()));
    }

    #[test]
    fn a_partly_nan_bucket_reports_the_finite_samples() {
        let t = ramp(4, 1.0);
        let v = [f64::NAN, 3.0, -1.0, f64::NAN];
        let env = min_max(&t, &v, 0, 3, 1);
        assert_eq!(env.min, vec![-1.0]);
        assert_eq!(env.max, vec![3.0]);
    }

    #[test]
    fn bucket_boundaries_split_the_remainder_at_the_end() {
        // n = 10, m = 3 -> edges 0, 3, 6, 10, i.e. sizes 3 / 3 / 4. Pinning
        // this pins the frontend's `Math.floor((b * n) / m)` exactly.
        let t = ramp(10, 1.0);
        let v = [0.0, 1.0, 2.0, 10.0, 11.0, 12.0, 20.0, 21.0, 22.0, 99.0];
        let env = min_max(&t, &v, 0, 9, 3);
        assert_eq!(env.min, vec![0.0, 10.0, 20.0]);
        assert_eq!(env.max, vec![2.0, 12.0, 99.0]);
        // Midpoint indices 1, 4, 7 — the last bucket spans 6..=9.
        assert_eq!(env.t, vec![1.0, 4.0, 7.0]);
    }

    #[test]
    fn bucket_time_is_the_midpoint_index_not_the_mean_time() {
        // Irregular sampling makes the two answers differ by a mile; that is
        // the whole point of the choice.
        let t = [0.0, 1.0, 100.0];
        let v = [5.0, 6.0, 7.0];
        let env = min_max(&t, &v, 0, 2, 1);
        assert_eq!(env.t, vec![1.0]); // the mean time would be 33.67
        assert_eq!(env.min, vec![5.0]);
        assert_eq!(env.max, vec![7.0]);
    }

    #[test]
    fn more_buckets_than_samples_gives_one_sample_per_bucket() {
        let t = ramp(5, 0.5);
        let v = [3.0, 1.0, 4.0, 1.0, 5.0];
        let env = min_max(&t, &v, 0, 4, 10_000);
        assert_eq!(env.len(), 5);
        assert_eq!(env.t, t);
        assert_eq!(env.min, v.to_vec());
        assert_eq!(env.max, v.to_vec());
    }

    #[test]
    fn zero_buckets_still_yields_one() {
        // `Math.max(1, ...)`: asking for nothing gets the whole range as a
        // single bucket, not an empty plot.
        let t = ramp(6, 1.0);
        let v = [1.0, 9.0, 2.0, 8.0, 3.0, 7.0];
        let env = min_max(&t, &v, 0, 5, 0);
        assert_eq!(env.len(), 1);
        assert_eq!(env.min, vec![1.0]);
        assert_eq!(env.max, vec![9.0]);
    }

    #[test]
    fn a_single_sample_range_is_a_single_bucket() {
        let t = ramp(5, 1.0);
        let v = [0.0, 10.0, 20.0, 30.0, 40.0];
        let env = min_max(&t, &v, 2, 2, 64);
        assert_eq!(env.t, vec![2.0]);
        assert_eq!(env.min, vec![20.0]);
        assert_eq!(env.max, vec![20.0]);
    }

    #[test]
    fn a_sub_range_ignores_everything_outside_it() {
        let t = ramp(8, 1.0);
        let v = [99.0, 99.0, 1.0, 2.0, 3.0, 4.0, 99.0, 99.0];
        let env = min_max(&t, &v, 2, 5, 2);
        assert_eq!(env.t, vec![2.0, 4.0]);
        assert_eq!(env.min, vec![1.0, 3.0]);
        assert_eq!(env.max, vec![2.0, 4.0]);
    }

    #[test]
    fn degenerate_ranges_return_an_empty_envelope() {
        // Where the Java throws. See the `min_max` doc comment.
        let t = ramp(4, 1.0);
        let v = [0.0, 1.0, 2.0, 3.0];
        assert!(min_max(&t, &v, 3, 2, 8).is_empty()); // inverted
        assert!(min_max(&[], &[], 0, 0, 8).is_empty()); // no data
        assert!(min_max(&t, &v, 9, 20, 8).is_empty()); // wholly past the end
    }

    #[test]
    fn an_over_long_range_clamps_to_the_data() {
        let t = ramp(4, 1.0);
        let v = [0.0, 1.0, 2.0, 3.0];
        let env = min_max(&t, &v, 0, 999, 1);
        assert_eq!(env.min, vec![0.0]);
        assert_eq!(env.max, vec![3.0]);
        // A short value array truncates the series instead of aborting.
        let env = min_max(&t, &v[..2], 0, 3, 1);
        assert_eq!(env.max, vec![1.0]);
    }

    #[test]
    fn infinities_are_samples_except_when_they_are_all_positive() {
        let t = ramp(2, 1.0);
        let env = min_max(&t, &[f64::NEG_INFINITY, 5.0], 0, 1, 1);
        assert_eq!(env.min, vec![f64::NEG_INFINITY]);
        assert_eq!(env.max, vec![5.0]);
        // The documented blind spot, shared with the Java and decimate.ts.
        let env = min_max(&t, &[f64::INFINITY, f64::INFINITY], 0, 1, 1);
        assert!(env.min[0].is_nan() && env.max[0].is_nan());
    }

    #[test]
    fn bucket_edges_are_computed_wide_enough_for_wasm32() {
        // `edge` widens to `u64` because `usize` is 32-bit on wasm32. This is
        // what a 32-bit multiply would have produced for a 100 k-sample
        // window at bucket 70 000: a wrapped, wrong boundary — and silently
        // so in a release build.
        let (b, n, m) = (70_000usize, 100_000usize, 1_000usize);
        assert_eq!(edge(b, n, m), 7_000_000);
        let wrapped = (b as u32).wrapping_mul(n as u32) / m as u32;
        assert_ne!(wrapped as usize, edge(b, n, m));
    }

    #[test]
    fn envelope_len_and_is_empty_agree() {
        let env = Envelope::default();
        assert!(env.is_empty());
        assert_eq!(env.len(), 0);
        let env = min_max(&ramp(3, 1.0), &[1.0, 2.0, 3.0], 0, 2, 3);
        assert!(!env.is_empty());
        assert_eq!(env.len(), 3);
    }
}
