// Measured-vs-simulated comparison (roadmap item 7): resample the simulated
// series onto the measured raster and reduce the residuals to error metrics.
// Pure functions, mirroring stats.ts style; gap discipline matches the
// backend's canonical sampling semantics — NaN endpoints are never bridged.

import { lowerBound } from './decimate'

/**
 * Linear interpolation at x on a sorted raster: exact hit returns that
 * sample; before the first sample there is nothing to interpolate → NaN;
 * past the last sample the value is held; a NaN on either bracketing
 * endpoint stays NaN — gaps are shown, not papered over.
 */
export function linearAt(t: Float64Array, v: Float64Array, x: number): number {
  if (t.length === 0) return Number.NaN
  const lb = lowerBound(t, x)
  if (lb < t.length && t[lb] === x) return v[lb]
  const i = lb - 1
  if (i < 0) return Number.NaN
  if (lb >= t.length) return v[t.length - 1]
  const t0 = t[i]
  const t1 = t[lb]
  const v0 = v[i]
  const v1 = v[lb]
  if (Number.isNaN(v0) || Number.isNaN(v1)) return Number.NaN
  return v0 + ((v1 - v0) * (x - t0)) / (t1 - t0)
}

/** Error metrics of simulated-vs-measured over the bound range. */
export interface CompareStats {
  /** Number of measured samples that produced a valid pair. */
  n: number
  rmse: number
  maxAbsError: number
  /** Time of the worst absolute error (jump-to-worst affordance). */
  maxAbsErrorAt: number
  /** Mean signed error, simulated − measured: positive = simulation reads high. */
  bias: number
  meanAbsError: number
}

/**
 * Walks the measured raster over [from, to] (null = unbounded), linearly
 * interpolates the simulated series at each measured timestamp, and reduces
 * the residuals sim − meas. Pairs where either side is NaN are skipped, and
 * measured samples outside the simulated series' own time span are excluded
 * — a comparison beyond what the simulation computed would score a frozen
 * hold-last tail, not the model. Returns null when no valid pair exists
 * (disjoint time ranges, empty rasters).
 */
export function compareStats(
  measT: Float64Array,
  measV: Float64Array,
  simT: Float64Array,
  simV: Float64Array,
  from: number | null,
  to: number | null,
): CompareStats | null {
  if (simT.length === 0) return null
  const start = from === null ? simT[0] : Math.max(from, simT[0])
  const end = to === null ? simT[simT.length - 1] : Math.min(to, simT[simT.length - 1])
  const lo = lowerBound(measT, start)
  let n = 0
  let sumSq = 0
  let sum = 0
  let sumAbs = 0
  let maxAbs = -1
  let maxAbsAt = Number.NaN
  for (let i = lo; i < measT.length; i++) {
    const x = measT[i]
    if (x > end) break
    const m = measV[i]
    if (Number.isNaN(m)) continue
    const s = linearAt(simT, simV, x)
    if (Number.isNaN(s)) continue
    const e = s - m
    n++
    sumSq += e * e
    sum += e
    const a = Math.abs(e)
    sumAbs += a
    if (a > maxAbs) {
      maxAbs = a
      maxAbsAt = x
    }
  }
  if (n === 0) return null
  return {
    n,
    rmse: Math.sqrt(sumSq / n),
    maxAbsError: maxAbs,
    maxAbsErrorAt: maxAbsAt,
    bias: sum / n,
    meanAbsError: sumAbs / n,
  }
}
