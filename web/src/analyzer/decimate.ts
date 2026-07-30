// Pure decimation kernels for the Data Analyzer (design contract §2.5d).
//
// Type-aware: analog channels get an M4-style per-bucket min/max envelope so
// no peak is ever lost; boolean/enum channels go through the same kernel with
// the guarantee spelled out in booleanEnvelope — a bucket containing any
// transition reports both levels (min=0 / max=1 for booleans), so a 1-sample
// pulse renders full-height instead of disappearing sub-pixel. Exactness under
// the point budget comes from the caller (ChannelStore returns raw samples,
// i.e. every edge, whenever the window fits the budget).
//
// Buckets are index-uniform (equal sample count per bucket) rather than
// time-uniform: robust to irregular sampling and never produces empty buckets.

export interface Envelope {
  t: Float64Array
  min: Float64Array
  max: Float64Array
}

/** First index i in [0, a.length) with a[i] >= x (a ascending). */
export function lowerBound(a: Float64Array, x: number): number {
  let lo = 0
  let hi = a.length
  while (lo < hi) {
    const mid = (lo + hi) >>> 1
    if (a[mid] < x) lo = mid + 1
    else hi = mid
  }
  return lo
}

/**
 * Min/max envelope of v over the inclusive index range [i0, i1], reduced to at
 * most `buckets` buckets. NaN samples (import gaps) are skipped; an all-NaN
 * bucket yields NaN min/max, which renders as a gap.
 */
export function minMaxEnvelope(
  t: Float64Array,
  v: Float64Array,
  i0: number,
  i1: number,
  buckets: number,
): Envelope {
  const n = i1 - i0 + 1
  const m = Math.max(1, Math.min(buckets, n))
  const outT = new Float64Array(m)
  const outMin = new Float64Array(m)
  const outMax = new Float64Array(m)
  for (let b = 0; b < m; b++) {
    const start = i0 + Math.floor((b * n) / m)
    const end = i0 + Math.floor(((b + 1) * n) / m) - 1
    let mn = Number.POSITIVE_INFINITY
    let mx = Number.NEGATIVE_INFINITY
    for (let i = start; i <= end; i++) {
      const x = v[i]
      if (Number.isNaN(x)) continue
      if (x < mn) mn = x
      if (x > mx) mx = x
    }
    outT[b] = t[start + ((end - start) >>> 1)]
    if (mn === Number.POSITIVE_INFINITY) {
      outMin[b] = Number.NaN
      outMax[b] = Number.NaN
    } else {
      outMin[b] = mn
      outMax[b] = mx
    }
  }
  return { t: outT, min: outMin, max: outMax }
}

/**
 * Transition-preserving decimation for boolean/enum channels (§2.5d). For 0/1
 * data the min/max pair *is* the per-bucket "any-change" flag: a bucket
 * containing a transition necessarily spans both levels (min=0, max=1), so no
 * pulse can vanish regardless of how far the view is zoomed out. Kept as a
 * distinct entry point so the semantic contract has a name, a call site, and
 * its own tests, even though the kernel is shared with the analog path.
 */
export function booleanEnvelope(
  t: Float64Array,
  v: Float64Array,
  i0: number,
  i1: number,
  buckets: number,
): Envelope {
  return minMaxEnvelope(t, v, i0, i1, buckets)
}
