// Pure range statistics + raster helpers for the Data Analyzer (Phase 2).
//
// The merged raster + step-hold pair here is shared by the Table instrument
// and the CSV exporter: the Table instrument interpolates empty cells by step-hold
// (a signal's value at time t is its last sample at or before t), and the
// same fill rule is what the export writes. Statistics bind to the cursor
// A–B range per design contract §2.5e.

import { lowerBound } from './decimate'

export interface RangeStats {
  count: number
  mean: number
  min: number
  max: number
  median: number
  /** Sample standard deviation (n−1); 0 for a single sample. */
  stddev: number
}

/**
 * Statistics of v over the time window [from, to] (inclusive; null = open
 * end). NaN samples (import gaps) are skipped; returns null when the window
 * holds no finite sample.
 */
export function rangeStats(
  t: Float64Array,
  v: Float64Array,
  from: number | null,
  to: number | null,
): RangeStats | null {
  const n = t.length
  if (n === 0) return null
  const i0 = from === null ? 0 : lowerBound(t, from)
  let i1 = to === null ? n - 1 : lowerBound(t, to)
  if (i1 < n && !(t[i1] <= (to ?? Infinity))) i1--
  if (i1 >= n) i1 = n - 1
  if (i0 > i1) return null

  const finite: number[] = []
  let sum = 0
  let min = Number.POSITIVE_INFINITY
  let max = Number.NEGATIVE_INFINITY
  for (let i = i0; i <= i1; i++) {
    const x = v[i]
    if (Number.isNaN(x)) continue
    finite.push(x)
    sum += x
    if (x < min) min = x
    if (x > max) max = x
  }
  const count = finite.length
  if (count === 0) return null
  const mean = sum / count

  let sq = 0
  for (const x of finite) sq += (x - mean) * (x - mean)
  const stddev = count > 1 ? Math.sqrt(sq / (count - 1)) : 0

  finite.sort((a, b) => a - b)
  const mid = count >>> 1
  const median = count % 2 === 1 ? finite[mid] : (finite[mid - 1] + finite[mid]) / 2

  return { count, mean, min, max, median, stddev }
}

/**
 * Merge several sorted time bases into one sorted, deduplicated raster.
 * Simple concat+sort — inputs are at most a few visible-window slices.
 */
export function mergeTimestamps(arrays: readonly Float64Array[]): Float64Array {
  let total = 0
  for (const a of arrays) total += a.length
  const all = new Float64Array(total)
  let off = 0
  for (const a of arrays) {
    all.set(a, off)
    off += a.length
  }
  all.sort()
  if (all.length === 0) return all
  let w = 1
  for (let i = 1; i < all.length; i++) {
    if (all[i] !== all[w - 1]) all[w++] = all[i]
  }
  return all.slice(0, w)
}

/**
 * Step-hold sample: the signal's value at the last sample with t[i] <= x.
 * Before the first sample there is nothing to hold → NaN (rendered blank).
 * A held NaN (gap sample) stays NaN — gaps are shown, not papered over.
 */
export function stepHoldAt(t: Float64Array, v: Float64Array, x: number): number {
  const lb = lowerBound(t, x)
  const i = lb < t.length && t[lb] === x ? lb : lb - 1
  return i < 0 ? Number.NaN : v[i]
}
