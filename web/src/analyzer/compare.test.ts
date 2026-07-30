import { describe, expect, it } from 'vitest'
import { compareStats, linearAt } from './compare'

const f64 = (a: number[]) => new Float64Array(a)

describe('linearAt', () => {
  const t = f64([0, 1, 2, 4])
  const v = f64([10, 20, 30, 50])

  it('returns the exact sample on a hit', () => {
    expect(linearAt(t, v, 2)).toBe(30)
  })

  it('interpolates between samples', () => {
    expect(linearAt(t, v, 0.5)).toBe(15)
    expect(linearAt(t, v, 3)).toBe(40) // halfway across the wide gap
  })

  it('is NaN before the first sample and held after the last', () => {
    expect(linearAt(t, v, -0.1)).toBeNaN()
    expect(linearAt(t, v, 99)).toBe(50)
  })

  it('never bridges a NaN gap', () => {
    const gv = f64([10, Number.NaN, 30, 50])
    expect(linearAt(t, gv, 0.5)).toBeNaN()
    expect(linearAt(t, gv, 1.5)).toBeNaN()
    expect(linearAt(t, gv, 1)).toBeNaN() // the exact NaN sample itself
    expect(linearAt(t, gv, 3)).toBe(40) // clean segment unaffected
  })

  it('handles an empty raster', () => {
    expect(linearAt(f64([]), f64([]), 1)).toBeNaN()
  })
})

describe('compareStats', () => {
  it('computes exact metrics on a known pair', () => {
    // measured: y = t on [0..4]; simulated: y = t + 1 exactly on its own raster
    const measT = f64([0, 1, 2, 3, 4])
    const measV = f64([0, 1, 2, 3, 4])
    const simT = f64([0, 4])
    const simV = f64([1, 5])
    const s = compareStats(measT, measV, simT, simV, null, null)
    expect(s).not.toBeNull()
    expect(s!.n).toBe(5)
    expect(s!.rmse).toBeCloseTo(1, 12)
    expect(s!.bias).toBeCloseTo(1, 12) // simulation reads high by +1
    expect(s!.maxAbsError).toBeCloseTo(1, 12)
    expect(s!.meanAbsError).toBeCloseTo(1, 12)
  })

  it('locates the worst error and respects the range bounds', () => {
    const measT = f64([0, 1, 2, 3])
    const measV = f64([0, 0, 0, 0])
    const simT = f64([0, 1, 2, 3])
    const simV = f64([0.1, -5, 0.2, 0.3])
    const all = compareStats(measT, measV, simT, simV, null, null)
    expect(all!.maxAbsError).toBeCloseTo(5, 12)
    expect(all!.maxAbsErrorAt).toBe(1)
    // Excluding the spike via the range changes the verdict.
    const bounded = compareStats(measT, measV, simT, simV, 1.5, 3)
    expect(bounded!.n).toBe(2)
    expect(bounded!.maxAbsError).toBeCloseTo(0.3, 12)
    expect(bounded!.maxAbsErrorAt).toBe(3)
  })

  it('skips NaN pairs and reports null on disjoint ranges', () => {
    const measT = f64([0, 1, 2])
    const measV = f64([1, Number.NaN, 3])
    const simT = f64([0, 2])
    const simV = f64([1, 3])
    const s = compareStats(measT, measV, simT, simV, null, null)
    expect(s!.n).toBe(2) // the NaN measured sample is skipped
    // Simulated series entirely before the measured range: not a comparison —
    // scoring a frozen hold-last tail would grade a value the model never
    // computed, so the overlap window excludes it entirely.
    const disjoint = compareStats(f64([10, 11]), f64([1, 2]), f64([0, 1]), f64([5, 6]), null, null)
    expect(disjoint).toBeNull()
    // Truly empty: empty sim raster.
    expect(compareStats(measT, measV, f64([]), f64([]), null, null)).toBeNull()
  })

  it('excludes the measured tail beyond the simulated span', () => {
    // sim covers [0, 2]; measurement runs to t = 10 — only the overlap counts.
    const measT = f64([0, 1, 2, 5, 10])
    const measV = f64([0, 0, 0, 0, 0])
    const simT = f64([0, 2])
    const simV = f64([1, 1])
    const s = compareStats(measT, measV, simT, simV, null, null)
    expect(s!.n).toBe(3)
    expect(s!.rmse).toBeCloseTo(1, 12)
  })
})
