//! Output-raster construction for calculated signals.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/measurement/MergedRaster.java`
//! (97 LOC). A calculated signal has to be evaluated *somewhere*, and the Java
//! offers three choices of time base: the **union** of the inputs' own rasters
//! ([`union`]), a **fixed** sample interval ([`fixed`]), or one named input's
//! raster verbatim — the third needs no code, the caller just hands that base
//! on unchanged.
//!
//! # The point cap is a guided path, not a refusal
//!
//! Merging two channels rastered at 100 kHz over five seconds is a perfectly
//! reasonable thing to ask for and a million-point answer; refusing it with a
//! bare "too big" leaves the user nowhere. So the cap is reported as
//! [`MeasurementError::RasterCapExceeded`] carrying a `suggested_dt` from
//! [`suggest_dt`] that **verifiably** lands under the cap, and the frontend
//! offers it as a one-click fix.
//!
//! The cap is checked **after** dedupe, and that ordering is load-bearing: ten
//! channels sharing one 10 kHz time master total ten times the raw samples but
//! merge to exactly one raster. Checking the concatenated total first would
//! refuse precisely the documents that motivate merging.
//!
//! # NaN times
//!
//! A `NaN` *time* can reach here. `ChannelData::time` is widened by the same
//! `AS_DOUBLE` path as the values, so a time master with an unreadable sample
//! arrives as `NaN` rather than as an absent point. Nothing upstream filters
//! it, and this module deliberately does not either — it is the caller's
//! corrupt file, and silently deleting samples from a measurement is worse than
//! propagating the corruption visibly. Concretely:
//!
//! * `NaN != NaN`, so **every** `NaN` time survives dedupe as its own point.
//!   Corrupt time data therefore inflates the point count (and can trip the
//!   cap) instead of collapsing into one silent sample — same as the Java.
//! * Sorting uses [`f64::total_cmp`], which puts negatively-signed `NaN` first
//!   and positively-signed `NaN` last; Java's `Arrays.sort` clusters all `NaN`
//!   at the end. Only the *position* of the `NaN` entries differs, never the
//!   count, so the cap decision is unaffected.
//! * [`suggest_dt`] returns `NaN` once a `NaN` reaches the span, because this
//!   module's `java_min`/`java_max` propagate it the way Java's do.
//!   Rust's own `f64::min`/`f64::max` ignore `NaN`, which would have handed the
//!   user a plausible-looking `dt` derived from a raster whose extent is not
//!   actually known.
//!
//! # Divergence from the Java
//!
//! `fixed` over an *infinite* span (`t0 = -inf`, or `t1 - t0` overflowing to
//! infinity) computes `n = (long) Math.floor(inf) + 1` in Java, which saturates
//! to `Long.MAX_VALUE` and then **overflows to `Long.MIN_VALUE`**; that is not
//! `> cap`, so the Java falls through to `new double[(int) Long.MIN_VALUE]` and
//! returns an *empty* raster. This port saturates instead, so the same input
//! reports the cap. Silently answering "no points" to a question about an
//! unbounded interval is a wrong answer, not a smaller one.
//!
//! [`union`] also parts company with the Java's `concatenate → Arrays.sort →
//! dedupe`, and for a reason that is purely about *this* runtime — see the
//! "Working memory" section on [`union`]. The answer is identical; the peak
//! allocation is not.

use core::cmp::Ordering;
use std::collections::BinaryHeap;

use super::{MeasurementError, Result};

/// Union of the input time bases: every distinct time, ascending.
///
/// `bases` are assumed non-decreasing — they are channel time masters. Two
/// things depend on that assumption, and neither changes the answer:
///
/// * the `suggested_dt` of the cap error takes the span from each base's first
///   and last sample rather than from a full scan, exactly as the Java does. An
///   unsorted base still merges correctly, it just may be offered a `dt` sized
///   for the wrong extent;
/// * a non-decreasing set of bases takes the streaming path below. An unsorted
///   one falls back to the Java's own concatenate-sort-dedupe, which is the
///   reference this path is checked against
///   (`union_matches_the_concatenating_reference_on_random_bases`).
///
/// # Working memory
///
/// The Java concatenates first and caps afterwards, and **that ordering is
/// load-bearing** — ten channels sharing one 10 kHz master total ten times the
/// raw samples but merge to exactly one raster, so checking the concatenated
/// total would refuse precisely the documents that motivate merging. What does
/// not transfer is paying for it in *memory*: the concatenation is
/// `8 · Σ|bases|` bytes and the cap only ever bounds the answer. Measured on
/// this port before the streaming path existed, with five 10 M-sample channels
/// at different rates and the boundary's own 1 M-point cap: **381 MB of peak
/// allocation and 5.7 s, to produce a refusal.** In a browser tab that shares
/// wasm32's 32-bit linear memory with the property tables, the component
/// library and up to 512 MiB of open recordings, an allocation that large fails
/// — and when it does, the user is told "could not be allocated" naming the
/// *concatenated* count, instead of `RASTER_CAP_EXCEEDED` naming the real point
/// count and a `dt` they can act on. A refusal that cannot be acted on is the
/// worse of the two failures.
///
/// So the sorted path merges the bases as streams and stops *storing* at the
/// cap while continuing to *count*, which keeps `actual_points` exactly what
/// the Java would have reported. Peak is `8 · min(Σ|bases|, cap)` — 8 MB at the
/// boundary's cap, whatever the inputs — and the merge is `O(Σn · log k)`
/// rather than a sort's `O(Σn · log Σn)`. The same workload afterwards: 8 MB
/// and 1.5 s.
pub fn union(bases: &[&[f64]], cap: u32) -> Result<Vec<f64>> {
    let mut total: u64 = 0;
    let mut t_min = f64::INFINITY;
    let mut t_max = f64::NEG_INFINITY;
    for b in bases {
        total = total.saturating_add(b.len() as u64);
        if let (Some(&first), Some(&last)) = (b.first(), b.last()) {
            t_min = java_min(t_min, first);
            t_max = java_max(t_max, last);
        }
    }

    // `total_cmp`, not `<=`: it is the order the merge and the reference sort
    // both use, so "sorted" here means exactly "the merge sees what the sort
    // would have produced". A base holding a `NaN` anywhere but its
    // positively-signed tail is therefore *not* sorted, and takes the fallback.
    let (mut all, distinct) = if bases
        .iter()
        .all(|b| b.is_sorted_by(|x, y| x.total_cmp(y) != Ordering::Greater))
    {
        merge_sorted(bases, total, cap)?
    } else {
        concat_sort(bases, total)?
    };

    if distinct > u64::from(cap) {
        return Err(MeasurementError::RasterCapExceeded {
            actual_points: distinct,
            suggested_dt: suggest_dt(t_min, t_max, cap),
            cap,
        });
    }
    // The caller holds this for the whole evaluation; a heavily-duplicated
    // merge would otherwise retain the pre-dedupe allocation.
    all.shrink_to_fit();
    Ok(all)
}

/// One base's current head, ordered so [`BinaryHeap`] — a *max*-heap — pops the
/// smallest time first.
///
/// Ties are broken by base index only to make [`Ord`] total; elements that
/// compare equal under [`f64::total_cmp`] are bit-identical, so which base a
/// duplicate came from can never be observed in the output.
struct Head {
    value: f64,
    base: usize,
}

impl Ord for Head {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .value
            .total_cmp(&self.value)
            .then_with(|| other.base.cmp(&self.base))
    }
}

impl PartialOrd for Head {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Head {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Head {}

/// Merge non-decreasing bases, keeping at most `cap` distinct times but
/// counting all of them.
///
/// Returns `(kept, distinct)`. `kept` is the answer when `distinct <= cap`, and
/// a truncated prefix that the caller discards otherwise; `distinct` is always
/// the full count, so the cap error reports the same number the Java's
/// post-dedupe `all.length` would have.
fn merge_sorted(bases: &[&[f64]], total: u64, cap: u32) -> Result<(Vec<f64>, u64)> {
    let mut out = alloc_exact(total.min(u64::from(cap)))?;
    let mut cursors = vec![0usize; bases.len()];
    let mut heap: BinaryHeap<Head> = BinaryHeap::with_capacity(bases.len());
    for (base, b) in bases.iter().enumerate() {
        if let Some(&value) = b.first() {
            heap.push(Head { value, base });
        }
    }

    let mut distinct: u64 = 0;
    let mut last: Option<f64> = None;
    while let Some(Head { value, base }) = heap.pop() {
        cursors[base] += 1;
        if let Some(&next) = bases[base].get(cursors[base]) {
            heap.push(Head { value: next, base });
        }
        // IEEE equality against the last *emitted* time, which is precisely
        // what `Vec::dedup` does to the sorted concatenation: `-0.0` swallows
        // the `0.0` behind it, and no `NaN` ever equals its predecessor, so
        // every corrupt time survives as its own point.
        let is_new = match last {
            Some(previous) => previous != value,
            None => true,
        };
        if is_new {
            last = Some(value);
            distinct += 1;
            if distinct <= u64::from(cap) {
                out.push(value);
            }
        }
    }
    Ok((out, distinct))
}

/// The Java's own path: concatenate, sort, drop adjacent duplicates.
///
/// Kept for bases that are not non-decreasing — a corrupt time master, or an
/// inline series the browser built out of order. Its peak really is the whole
/// concatenation, which is why [`union`] only reaches it when the streaming
/// merge would not answer the same question. [`alloc_exact`] makes an
/// impossible size a typed error rather than an abort.
fn concat_sort(bases: &[&[f64]], total: u64) -> Result<(Vec<f64>, u64)> {
    let mut all = alloc_exact(total)?;
    for b in bases {
        all.extend_from_slice(b);
    }
    // `total_cmp` is a total order, so elements that compare equal are
    // bit-identical and sort stability cannot be observed. Unstable is both
    // faster and what Java's dual-pivot quicksort does.
    all.sort_unstable_by(f64::total_cmp);
    // IEEE equality on purpose: it folds -0.0 into 0.0 and keeps every NaN,
    // exactly as the Java's `all[i] != all[w - 1]` does.
    all.dedup();
    let distinct = all.len() as u64;
    Ok((all, distinct))
}

/// Fixed-interval raster `t0, t0 + dt, …` covering `[t0, t1]`.
///
/// Rejects a non-positive or `NaN` `dt` and a reversed or `NaN` interval as a
/// hard [`MeasurementError::Parse`] — the Java's `IllegalArgumentException`.
/// That is a different thing from the cap: bad arguments have no suggested
/// remedy to offer.
///
/// An *infinite* `dt` passes the `dt > 0` guard and yields exactly one point,
/// which is `t0 + 0 * inf` — that is, `NaN`, in this port and in the Java
/// alike. Kept rather than special-cased: the arithmetic is the Java's, and a
/// `NaN` time is this module's visible signal for garbage, not a silent one.
pub fn fixed(t0: f64, t1: f64, dt: f64, cap: u32) -> Result<Vec<f64>> {
    // The Java guard is `!(dt > 0) || !(t1 >= t0)` — negated on purpose so that
    // a NaN falls into the error arm instead of sliding past a `<=` test.
    // Spelled out here because clippy rejects the negated form on a partially
    // ordered type; the truth tables are identical.
    let bad_dt = dt.is_nan() || dt <= 0.0;
    // `t1 - t0` is `NaN` when both endpoints are the *same* infinity, which
    // slips past `t1 < t0` — and a channel's decoded time master can carry an
    // infinity, so this is reachable from a file rather than only from a
    // caller. It has to be refused here rather than left to the arithmetic
    // below: `(NaN + 1.0) as u64` saturates to **0**, so `fixed` would answer
    // an *empty* raster and the calc above it would report a successful,
    // empty column for a question that has no answer. (The Java's
    // `(long) Math.floor(NaN) + 1` is 1, so it answers a single point at
    // infinity — different garbage, equally unhelpful, and the explicit
    // refusal beats both.)
    let bad_interval = t0.is_nan() || t1.is_nan() || t1 < t0 || (t1 - t0).is_nan();
    if bad_dt || bad_interval {
        return Err(MeasurementError::Parse(format!(
            "A fixed raster needs dt > 0 and a finite span with t1 >= t0; got t0 = {t0}, \
             t1 = {t1}, dt = {dt}."
        )));
    }

    // The `+ 1` happens in integers, not in `f64`, because the Java's
    // `(long) Math.floor(x) + 1` casts first — and once `floor(x)` is past
    // 2^53 an `f64` `+ 1.0` is a *no-op*. Written the other way round,
    // `fixed(0, 1e12, 1e-6, cap)` reported 1 000 000 000 000 000 000 points
    // where the oracle reports 1 000 000 000 000 000 001. The cap *decision* is
    // the same either way (a count that large clears any `u32` cap), but the
    // number is quoted verbatim in the message the user reads, and a refusal
    // that miscounts is a refusal the user cannot check.
    // `as u64` saturates rather than wrapping — see the module's divergence
    // note for what the Java's `long` does instead.
    let n = (libm::floor((t1 - t0) / dt) as u64).saturating_add(1);
    if n > u64::from(cap) {
        return Err(MeasurementError::RasterCapExceeded {
            actual_points: n,
            suggested_dt: suggest_dt(t0, t1, cap),
            cap,
        });
    }

    let mut out = alloc_exact(n)?;
    for i in 0..n {
        out.push(t0 + i as f64 * dt);
    }
    Ok(out)
}

/// Smallest 1-2-5 decade step whose fixed raster over `[t0, t1]` fits `cap`.
///
/// `dt` is `span / (cap - 1)` rounded **up** to the next 1-2-5-10 rung, so
/// `floor(span / dt) + 1 <= cap` holds by construction — the suggestion the cap
/// error carries is one the user can act on without a second round trip.
///
/// Degenerate caps are the Java's arithmetic, preserved, and **neither
/// suggestion is actionable**: `cap == 1` divides by zero and yields `+inf`,
/// which does give a one-point raster but whose one point is `NaN` (see
/// [`fixed`]); `cap == 0` divides by −1 and yields `NaN`, which is at least
/// honest, since no `dt` produces a zero-point raster. Both only arise on an
/// error path where nothing could have been returned anyway, so the Java never
/// noticed and neither does the frontend.
pub fn suggest_dt(t0: f64, t1: f64, cap: u32) -> f64 {
    let span = java_max(t1 - t0, 0.0);
    if span == 0.0 {
        // Nothing to divide. 1 ms is the Java's arbitrary but usable floor.
        return 1e-3;
    }
    let raw = span / (f64::from(cap) - 1.0);
    let decade = pow10(libm::floor(libm::log10(raw)));
    for m in [1.0, 2.0, 5.0, 10.0] {
        if m * decade >= raw {
            return m * decade;
        }
    }
    // Unreachable for a finite `raw`; a NaN span lands here and stays NaN.
    10.0 * decade
}

/// `10^k` for the integral `k` [`suggest_dt`] gets out of `floor(log10(raw))`,
/// correctly rounded.
///
/// **`libm::pow(10.0, k)` is not correctly rounded**, and Java's `Math.pow` is.
/// Checked against `Double.parseDouble("1e" + k)` — correctly rounded by
/// definition — `Math.pow` is exact at all sixty-one decades in `[-30, 30]`
/// while `libm::pow` (and Java's own `StrictMath.pow`, the same fdlibm
/// lineage) is one ULP out at eight of them: `k` = −29, −24, −21, −20, −17,
/// −11, −5 and 29.
///
/// Two of those errors point *downwards*, and there the damage is not a
/// last-digit wobble. The ladder below tests `1 * decade >= raw`, so a decade
/// one ULP under `raw` fails the rung it should have matched and the answer
/// jumps to the next one: `suggest_dt(0, 1e-4, 11)` came back as 2 × 10⁻⁵
/// where the Java says 10⁻⁵ — half the resolution the user was entitled to,
/// rendered as `0.00002` in the "use this dt instead" button.
///
/// A table of decimal literals is the fix rather than more arithmetic, because
/// Rust's literal parser is correctly rounded: each entry *is* the double
/// `Math.pow` produced.
///
/// The table stops at ±30 because that is every decade a raster over a time
/// master in *seconds* can produce. It is not, however, every decade
/// `suggest_dt` can be *called* with — it is `pub`, `cap` reaches 10⁶, and
/// `floor(log10(raw))` ranges over `[-324, 308]` for any positive finite `raw`.
/// Deferring the rest to `libm::pow` was once written off here as "can still be
/// an ULP out, which is accepted"; a bit-exact sweep against the Java says
/// otherwise. At `k = -32` — the *first* decade past the table —
/// `suggest_dt(0, 1e-30, 101)` answered 2 × 10⁻³² against the oracle's 10⁻³²,
/// and `suggest_dt(0, 2e-30, 101)` answered 5 × 10⁻³² against 2 × 10⁻³². That
/// is the same skipped rung the table was introduced to close, not a
/// last-digit wobble, and 54 points of a decade grid took it.
///
/// So the decades outside the table come from the same correctly-rounded
/// parser, at run time. Formatting a six-character string to obtain a power of
/// ten looks absurd next to a `pow` call, and is exactly right here:
/// `suggest_dt` runs **once, on the error path**, after a raster has already
/// been refused — the alternative is 570-odd more literals for decades no
/// measurement will ever reach. `±∞` (from `cap == 1`) and `NaN` (from a
/// corrupt span) fall past both and must reach `libm::pow` to stay themselves.
///
/// The claim underneath all of this, checked at all 633 integer decades rather
/// than assumed: Java's `Math.pow(10, k)` equals `Double.parseDouble("1e" + k)`
/// — correctly rounded by definition — at every one, while `StrictMath.pow`,
/// the fdlibm lineage `libm::pow` shares, differs at 64 of them.
fn pow10(k: f64) -> f64 {
    // `contains` is `start <= k && k <= end`, so NaN falls through — the point.
    if (-30.0..=30.0).contains(&k) {
        const POW10: [f64; 61] = [
            1e-30, 1e-29, 1e-28, 1e-27, 1e-26, 1e-25, 1e-24, 1e-23, 1e-22, 1e-21, 1e-20, 1e-19,
            1e-18, 1e-17, 1e-16, 1e-15, 1e-14, 1e-13, 1e-12, 1e-11, 1e-10, 1e-9, 1e-8, 1e-7, 1e-6,
            1e-5, 1e-4, 1e-3, 1e-2, 1e-1, 1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10,
            1e11, 1e12, 1e13, 1e14, 1e15, 1e16, 1e17, 1e18, 1e19, 1e20, 1e21, 1e22, 1e23, 1e24,
            1e25, 1e26, 1e27, 1e28, 1e29, 1e30,
        ];
        return POW10[(k as i32 + 30) as usize];
    }
    // `k` is `floor(log10(..))` of a finite positive double here, so it is
    // integral and `as i32` is exact within these bounds.
    if (-324.0..=308.0).contains(&k) {
        if let Ok(exact) = format!("1e{}", k as i32).parse::<f64>() {
            return exact;
        }
    }
    libm::pow(10.0, k)
}

/// `Vec::with_capacity` that reports failure instead of aborting.
///
/// Rust aborts the process on allocation failure, and on wasm32 that kills the
/// tab with no diagnostic — the failure mode the Phase 7–8 sweep found twice.
/// Both raster sizes are caller-influenced (`cap` is a `u32`, and the input
/// bases come straight out of a file), and on wasm32 `usize` is 32-bit, so the
/// concatenated total is not even guaranteed to be expressible.
fn alloc_exact(points: u64) -> Result<Vec<f64>> {
    let mut v: Vec<f64> = Vec::new();
    let fits = match usize::try_from(points) {
        Ok(n) => v.try_reserve_exact(n).is_ok(),
        Err(_) => false,
    };
    if !fits {
        return Err(MeasurementError::Parse(format!(
            "The raster needs {points} time points ({} bytes), which could not be allocated.",
            points.saturating_mul(8)
        )));
    }
    Ok(v)
}

/// `Math.min` (NaN-propagating, `-0.0 < 0.0`).
fn java_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else if b < a {
        b
    } else if a.is_sign_negative() {
        a
    } else {
        b
    }
}

/// `Math.max` (NaN-propagating, `0.0 > -0.0`).
fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else if b > a {
        b
    } else if a.is_sign_positive() {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `n` points at spacing `dt` starting at `t0`.
    fn ramp(n: usize, dt: f64, t0: f64) -> Vec<f64> {
        (0..n).map(|i| t0 + i as f64 * dt).collect()
    }

    /// A tiny deterministic generator so the property tests are reproducible
    /// without a `rand` dependency.
    struct TestRng(u64);
    impl TestRng {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 11
        }
    }

    /// The property the whole cap story rests on: the suggested `dt` must yield
    /// a fixed raster that actually fits.
    fn assert_suggestion_fits(t0: f64, t1: f64, cap: u32, dt: f64) {
        let points = (libm::floor((t1 - t0) / dt) + 1.0) as u64;
        assert!(
            points <= u64::from(cap),
            "suggested dt {dt} over [{t0}, {t1}] yields {points} points, cap {cap}"
        );
    }

    // ------------------------------------------------------------------
    // union
    // ------------------------------------------------------------------

    #[test]
    fn union_merges_sorts_and_dedupes() {
        // The Java's own vector (TimeSeriesEvaluatorTest).
        let a = [0.0, 2.0, 4.0];
        let b = [1.0, 2.0, 3.0];
        let merged = union(&[&a[..], &b[..], &[]], 100).unwrap();
        assert_eq!(merged, vec![0.0, 1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn union_dedupes_times_coincident_across_many_bases() {
        // Three channels on one shared 10 Hz master plus one offset channel:
        // 40 raw samples, 20 distinct times.
        let shared = ramp(10, 0.1, 0.0);
        let offset = ramp(10, 0.1, 0.05);
        let merged = union(&[&shared[..], &shared[..], &shared[..], &offset[..]], 100).unwrap();
        assert_eq!(merged.len(), 20);
        for w in merged.windows(2) {
            assert!(w[0] < w[1], "not strictly increasing at {w:?}");
        }
    }

    #[test]
    fn union_checks_the_cap_after_dedupe_not_before() {
        // 400 raw samples that merge to 100. Checking the concatenated total
        // first would refuse this, which is the normal case, not a pathology.
        let base = ramp(100, 0.01, 0.0);
        let merged = union(&[&base[..], &base[..], &base[..], &base[..]], 100).unwrap();
        assert_eq!(merged.len(), 100);
    }

    #[test]
    fn union_cap_boundary_passes_at_cap_and_fails_one_past_it() {
        let at_cap = ramp(100, 1.0, 0.0);
        assert_eq!(union(&[&at_cap[..]], 100).unwrap().len(), 100);

        let over_cap = ramp(101, 1.0, 0.0);
        match union(&[&over_cap[..]], 100) {
            Err(MeasurementError::RasterCapExceeded {
                actual_points,
                suggested_dt,
                cap,
            }) => {
                assert_eq!(actual_points, 101);
                assert_eq!(cap, 100);
                assert_suggestion_fits(0.0, 100.0, 100, suggested_dt);
            }
            other => panic!("expected a cap error, got {other:?}"),
        }
    }

    #[test]
    fn union_over_cap_carries_a_compliant_suggested_dt() {
        // The Java's overCapCarriesACompliantSuggestedDt: two offset 1 kHz
        // bases that exceed a 1000-point cap organically.
        let a = ramp(5000, 0.001, 0.0);
        let b = ramp(5000, 0.001, 0.0005);
        match union(&[&a[..], &b[..]], 1000) {
            Err(MeasurementError::RasterCapExceeded {
                actual_points,
                suggested_dt,
                ..
            }) => {
                assert_eq!(actual_points, 10_000);
                assert_suggestion_fits(0.0, 4.9995, 1000, suggested_dt);
            }
            other => panic!("expected a cap error, got {other:?}"),
        }
    }

    #[test]
    fn union_of_nothing_is_empty() {
        assert!(union(&[], 10).unwrap().is_empty());
        assert!(union(&[&[]], 10).unwrap().is_empty());
        assert!(union(&[&[], &[], &[]], 10).unwrap().is_empty());
        // Zero points is not over a zero cap.
        assert!(union(&[&[]], 0).unwrap().is_empty());
        assert!(union(&[&[1.0]], 0).is_err());
    }

    #[test]
    fn union_folds_signed_zero_and_keeps_every_nan() {
        // -0.0 == 0.0, so they dedupe; total_cmp orders -0.0 first, so that is
        // the representative that survives — as in the Java.
        let merged = union(&[&[-0.0, 0.0][..], &[0.0][..]], 10).unwrap();
        assert_eq!(merged.len(), 1);
        assert!(merged[0].is_sign_negative());

        // NaN never equals NaN: three corrupt times stay three points.
        let merged = union(&[&[0.0, f64::NAN][..], &[f64::NAN, f64::NAN][..]], 10).unwrap();
        assert_eq!(merged.len(), 4);
        assert_eq!(merged.iter().filter(|t| t.is_nan()).count(), 3);

        // Sorted-then-deduped against the oracle run directly on the Java
        // source: [NaN, 0.0, -0.0, 2.0, NaN] → [-0.0, 2.0, NaN, NaN].
        let merged = union(&[&[f64::NAN, 0.0, -0.0, 2.0, f64::NAN][..]], 10).unwrap();
        assert_eq!(merged.len(), 4);
        assert!(merged[0] == 0.0 && merged[0].is_sign_negative());
        assert_eq!(merged[1], 2.0);
        assert!(merged[2].is_nan() && merged[3].is_nan());
    }

    #[test]
    fn union_with_a_nan_time_suggests_a_nan_dt_rather_than_a_plausible_one() {
        let corrupt = [0.0, 1.0, f64::NAN];
        match union(&[&corrupt[..]], 2) {
            Err(MeasurementError::RasterCapExceeded {
                actual_points,
                suggested_dt,
                ..
            }) => {
                assert_eq!(actual_points, 3);
                assert!(
                    suggested_dt.is_nan(),
                    "NaN must propagate, got {suggested_dt}"
                );
            }
            other => panic!("expected a cap error, got {other:?}"),
        }
    }

    /// The streaming merge exists for its peak allocation, not for its answer —
    /// so its answer has to be the concatenate-sort-dedupe one, bit for bit,
    /// including the count it reports over the cap.
    ///
    /// The alphabet is deliberately narrow (a dozen values over five bases) so
    /// coincident times, repeats *within* one base, `-0.0`/`0.0` pairs and
    /// `NaN` all occur constantly rather than as a lucky draw. Each trial is
    /// checked at three caps, two of them straddling the exact point count,
    /// because the boundary is where a merge that miscounts by one would hide.
    #[test]
    fn union_matches_the_concatenating_reference_on_random_bases() {
        let mut rng = TestRng(0x5eed_0f11);
        for trial in 0..300 {
            let owned: Vec<Vec<f64>> = (0..1 + (rng.next() as usize % 5))
                .map(|_| {
                    let len = rng.next() as usize % 30;
                    let mut b: Vec<f64> = (0..len)
                        .map(|_| match rng.next() % 12 {
                            0 => f64::NAN,
                            1 => -0.0,
                            // Spans -0.75 ..= 1.5 and includes +0.0 at k = 5,
                            // so the -0.0/+0.0 fold is exercised every trial.
                            k => (k as f64 - 5.0) * 0.25,
                        })
                        .collect();
                    // Sorted under the same order the merge uses, so `union`
                    // takes the streaming path rather than the fallback.
                    b.sort_unstable_by(f64::total_cmp);
                    b
                })
                .collect();
            let bases: Vec<&[f64]> = owned.iter().map(Vec::as_slice).collect();
            assert!(
                bases
                    .iter()
                    .all(|b| b.is_sorted_by(|x, y| x.total_cmp(y) != Ordering::Greater)),
                "trial {trial} would take the fallback, making the comparison vacuous"
            );

            let total: u64 = bases.iter().map(|b| b.len() as u64).sum();
            let (reference, distinct) = concat_sort(&bases, total).expect("reference allocates");

            let caps = [
                distinct as u32,
                distinct.saturating_sub(1) as u32,
                rng.next() as u32 % 40,
            ];
            for cap in caps {
                let at = format!("trial {trial}, cap {cap}, {distinct} distinct");
                let got = union(&bases, cap);
                if distinct > u64::from(cap) {
                    match got {
                        Err(MeasurementError::RasterCapExceeded { actual_points, .. }) => {
                            assert_eq!(actual_points, distinct, "{at}");
                        }
                        other => panic!("{at}: expected a cap error, got {other:?}"),
                    }
                } else {
                    // Bits, not `==`: `NaN != NaN` would pass vacuously and
                    // `-0.0 == 0.0` would not catch the wrong survivor.
                    let got = got.unwrap_or_else(|e| panic!("{at}: {e:?}"));
                    let got_bits: Vec<u64> = got.iter().map(|t| t.to_bits()).collect();
                    let want_bits: Vec<u64> = reference.iter().map(|t| t.to_bits()).collect();
                    assert_eq!(got_bits, want_bits, "{at}");
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // fixed
    // ------------------------------------------------------------------

    #[test]
    fn fixed_respects_dt_and_cap() {
        // The Java's fixedRasterRespectsDtAndCap.
        let r = fixed(0.0, 1.0, 0.25, 100).unwrap();
        assert_eq!(r.len(), 5);
        assert!((r[3] - 0.75).abs() < 1e-12);
        assert_eq!(r[0], 0.0);
        assert_eq!(r[4], 1.0);

        assert!(matches!(
            fixed(0.0, 10.0, 1e-4, 1000),
            Err(MeasurementError::RasterCapExceeded { .. })
        ));
    }

    #[test]
    fn fixed_cap_boundary_passes_at_cap_and_fails_one_past_it() {
        // [0, 99] at dt = 1 is exactly 100 points.
        assert_eq!(fixed(0.0, 99.0, 1.0, 100).unwrap().len(), 100);
        match fixed(0.0, 100.0, 1.0, 100) {
            Err(MeasurementError::RasterCapExceeded {
                actual_points,
                suggested_dt,
                cap,
            }) => {
                assert_eq!(actual_points, 101);
                assert_eq!(cap, 100);
                assert_suggestion_fits(0.0, 100.0, 100, suggested_dt);
            }
            other => panic!("expected a cap error, got {other:?}"),
        }
    }

    #[test]
    fn fixed_zero_span_is_one_point() {
        let r = fixed(2.5, 2.5, 0.1, 10).unwrap();
        assert_eq!(r, vec![2.5]);
    }

    #[test]
    fn fixed_rejects_bad_arguments_as_a_hard_error() {
        // Not a cap signal — these have no remedy to suggest.
        for (t0, t1, dt) in [
            (0.0, 1.0, 0.0),
            (0.0, 1.0, -0.1),
            (0.0, 1.0, f64::NAN),
            (1.0, 0.0, 0.1),
            (f64::NAN, 1.0, 0.1),
            (0.0, f64::NAN, 0.1),
            // Both endpoints the same infinity: `t1 >= t0` holds, but the span
            // is NaN and no raster covers it. Refused rather than answered with
            // the empty vector `(NaN + 1.0) as u64 == 0` would have produced.
            (f64::INFINITY, f64::INFINITY, 1.0),
            (f64::NEG_INFINITY, f64::NEG_INFINITY, 1.0),
        ] {
            match fixed(t0, t1, dt, 100) {
                Err(MeasurementError::Parse(m)) => {
                    assert!(m.contains("dt > 0 and a finite span"), "{m}");
                }
                other => panic!("expected a Parse error for ({t0}, {t1}, {dt}), got {other:?}"),
            }
        }
    }

    #[test]
    fn fixed_over_an_infinite_span_reports_the_cap_not_an_empty_raster() {
        // The documented divergence: the Java's `(long) floor(inf) + 1`
        // overflows to Long.MIN_VALUE and returns an empty array.
        assert!(matches!(
            fixed(f64::NEG_INFINITY, f64::INFINITY, 1.0, 1000),
            Err(MeasurementError::RasterCapExceeded { .. })
        ));
        // Finite endpoints whose difference still overflows to infinity.
        assert!(matches!(
            fixed(-1e308, 1e308, 1.0, 1000),
            Err(MeasurementError::RasterCapExceeded { .. })
        ));
        // An infinite dt is legal (`dt > 0`) and collapses to one point — but
        // that point is `t0 + 0 * inf`, i.e. NaN. The Java does the same.
        let r = fixed(0.0, 1.0, f64::INFINITY, 10).unwrap();
        assert_eq!(r.len(), 1);
        assert!(r[0].is_nan(), "got {r:?}");
    }

    // ------------------------------------------------------------------
    // suggest_dt
    // ------------------------------------------------------------------

    #[test]
    fn suggest_dt_lands_on_each_rung_of_the_ladder() {
        // cap = 11 → raw = span / 10.
        assert_eq!(suggest_dt(0.0, 10.0, 11), 1.0); // raw 1.0 → rung 1
        assert_eq!(suggest_dt(0.0, 15.0, 11), 2.0); // raw 1.5 → rung 2
        assert_eq!(suggest_dt(0.0, 30.0, 11), 5.0); // raw 3.0 → rung 5
        assert_eq!(suggest_dt(0.0, 70.0, 11), 10.0); // raw 7.0 → rung 10

        // The Java's own case: raw = 0.99 rounds up past 0.5 to the next
        // decade. The oracle prints exactly 1.0 for this.
        assert_eq!(suggest_dt(0.0, 9.9, 11), 1.0);
    }

    #[test]
    fn suggest_dt_ladder_works_below_the_unit_decade() {
        // Oracle: 2.0E-4 and 5.0E-7. Exact equality holds because
        // libm::pow(10, k) agrees with Math.pow(10, k) on these decades.
        assert_eq!(suggest_dt(0.0, 1.5e-3, 11), 2e-4); // raw 1.5e-4 → rung 2
        assert_eq!(suggest_dt(0.0, 4e-6, 11), 5e-7); // raw 4e-7 → rung 5
    }

    /// The case [`pow10`] exists for. `libm::pow(10, -5)` is one ULP *below*
    /// 1e-5, so the `1 * decade >= raw` rung misses and the ladder hands the
    /// user 2e-5 — half the resolution they were entitled to, in a button that
    /// says "use this dt instead". Oracle (`MergedRaster.suggestDt`): `1.0E-5`.
    #[test]
    fn suggest_dt_does_not_skip_a_rung_on_an_inexact_decade() {
        assert_eq!(suggest_dt(0.0, 1e-4, 11), 1e-5);
    }

    /// Guards the table itself: a mistyped entry would be invisible above,
    /// since only eight of the sixty-one decades differ from `libm::pow` at
    /// all. Rust's literal parser is correctly rounded, so each literal *is*
    /// the double `Math.pow` produces.
    #[test]
    fn pow10_is_correctly_rounded_across_its_table() {
        for k in -30..=30i32 {
            let want: f64 = format!("1e{k}").parse().expect("a decimal literal");
            assert_eq!(pow10(k as f64).to_bits(), want.to_bits(), "10^{k}");
        }
        // The eight decades where deferring to `libm::pow` would be wrong — the
        // measurement behind the doc comment, kept executable.
        let differs: Vec<i32> = (-30..=30)
            .filter(|k| pow10(f64::from(*k)).to_bits() != libm::pow(10.0, f64::from(*k)).to_bits())
            .collect();
        assert_eq!(differs, vec![-29, -24, -21, -20, -17, -11, -5, 29]);
        // Outside the table the fallback must stay reachable and stay itself.
        assert!(pow10(f64::NAN).is_nan());
        assert_eq!(pow10(f64::INFINITY), f64::INFINITY);
        assert_eq!(pow10(31.0), 1e31);
    }

    #[test]
    fn suggest_dt_of_a_degenerate_span_is_one_millisecond() {
        assert_eq!(suggest_dt(5.0, 5.0, 100), 1e-3);
        assert_eq!(suggest_dt(0.0, 0.0, 100), 1e-3);
        // Reversed: Math.max(span, 0) clamps to zero rather than going negative.
        assert_eq!(suggest_dt(9.0, 1.0, 100), 1e-3);
        assert_eq!(suggest_dt(-0.0, 0.0, 100), 1e-3);
    }

    #[test]
    fn suggest_dt_always_fits_the_cap() {
        for &span in &[1e-6, 1e-3, 0.99, 1.0, 4.9995, 60.0, 3600.0, 1e6] {
            for &cap in &[2u32, 10, 11, 100, 1000, 1_000_000] {
                assert_suggestion_fits(0.0, span, cap, suggest_dt(0.0, span, cap));
            }
        }
    }

    #[test]
    fn suggest_dt_degenerate_caps_are_the_javas_arithmetic() {
        // cap 1: span / 0 → +inf. That step does yield 1 point, but a NaN one.
        assert_eq!(suggest_dt(0.0, 10.0, 1), f64::INFINITY);
        let one = fixed(0.0, 10.0, f64::INFINITY, 1).unwrap();
        assert_eq!(one.len(), 1);
        assert!(one[0].is_nan());
        // cap 0: span / -1 → log10 of a negative → NaN. No dt can satisfy it.
        assert!(suggest_dt(0.0, 10.0, 0).is_nan());
        // A zero span still short-circuits before either.
        assert_eq!(suggest_dt(3.0, 3.0, 0), 1e-3);
    }

    #[test]
    fn suggest_dt_propagates_nan() {
        assert!(suggest_dt(f64::NAN, 1.0, 100).is_nan());
        assert!(suggest_dt(0.0, f64::NAN, 100).is_nan());
        // Rust's own f64::max would have returned 0.0 here and reported 1 ms.
        assert!(suggest_dt(f64::NAN, f64::NAN, 100).is_nan());
    }

    // ------------------------------------------------------------------
    // helpers
    // ------------------------------------------------------------------

    #[test]
    fn java_min_max_propagate_nan_where_rust_would_not() {
        assert!(java_min(f64::NAN, 1.0).is_nan());
        assert!(java_max(1.0, f64::NAN).is_nan());
        assert_eq!(f64::NAN.max(1.0), 1.0); // the divergence being guarded
        assert_eq!(java_min(-0.0, 0.0), 0.0);
        assert!(java_min(-0.0, 0.0).is_sign_negative());
        assert!(java_max(-0.0, 0.0).is_sign_positive());
    }

    #[test]
    fn alloc_exact_reports_an_impossible_size_instead_of_aborting() {
        // 2^60 f64 is 8 EiB; no allocator satisfies it, and on wasm32 the
        // length does not even fit in a usize.
        match alloc_exact(1 << 60) {
            Err(MeasurementError::Parse(m)) => assert!(m.contains("could not be allocated"), "{m}"),
            other => panic!("expected a Parse error, got {other:?}"),
        }
        assert_eq!(alloc_exact(0).unwrap().capacity(), 0);
    }

    #[test]
    fn cap_error_renders_the_frontends_message() {
        let e = union(&[&[0.0, 1.0, 2.0][..]], 2).unwrap_err();
        assert_eq!(e.code(), "RASTER_CAP_EXCEEDED");
        let text = e.to_string();
        assert!(text.contains("3 points"), "{text}");
        assert!(text.contains("2-point cap"), "{text}");
    }
}
