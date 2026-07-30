// Range statistics + raster/step-hold contract tests (todo.md Phase 2).

import { describe, expect, it } from 'vitest'
import { mergeTimestamps, rangeStats, stepHoldAt } from './stats'

const t = new Float64Array([0, 1, 2, 3, 4, 5])
const v = new Float64Array([10, 20, 30, 40, 50, 60])

describe('rangeStats', () => {
  it('computes stats over the full array', () => {
    const s = rangeStats(t, v, null, null)
    expect(s).not.toBeNull()
    expect(s?.count).toBe(6)
    expect(s?.mean).toBe(35)
    expect(s?.min).toBe(10)
    expect(s?.max).toBe(60)
    expect(s?.median).toBe(35) // even count → midpoint of 30 and 40
  })

  it('binds to a [from, to] window inclusively', () => {
    const s = rangeStats(t, v, 1, 3)
    expect(s?.count).toBe(3)
    expect(s?.min).toBe(20)
    expect(s?.max).toBe(40)
    expect(s?.median).toBe(30)
  })

  it('skips NaN gaps and returns null for empty/NaN-only windows', () => {
    const vv = new Float64Array([10, NaN, 30, NaN, 50, 60])
    const s = rangeStats(t, vv, 0, 3)
    expect(s?.count).toBe(2)
    expect(s?.mean).toBe(20)
    expect(rangeStats(t, vv, 3, 3.5)).toBeNull()
    expect(rangeStats(t, v, 99, 100)).toBeNull()
  })

  it('sample stddev matches the analytic value', () => {
    const s = rangeStats(t, v, null, null)
    // values 10..60 step 10, mean 35: Σ(x−μ)² = 2·(625+225+25) = 1750,
    // sample stddev = sqrt(1750/5) = sqrt(350)
    expect(s?.stddev).toBeCloseTo(Math.sqrt(350), 12)
    expect(rangeStats(t, v, 2, 2)?.stddev).toBe(0) // single sample
  })
})

describe('mergeTimestamps', () => {
  it('merges, sorts and dedupes several rasters', () => {
    const merged = mergeTimestamps([
      new Float64Array([0, 2, 4]),
      new Float64Array([1, 2, 3]),
      new Float64Array([]),
    ])
    expect(Array.from(merged)).toEqual([0, 1, 2, 3, 4])
  })

  it('handles the empty case', () => {
    expect(mergeTimestamps([]).length).toBe(0)
  })
})

describe('stepHoldAt', () => {
  it('holds the last sample at or before x', () => {
    expect(stepHoldAt(t, v, 2)).toBe(30) // exact hit
    expect(stepHoldAt(t, v, 2.9)).toBe(30) // between samples → hold
    expect(stepHoldAt(t, v, 99)).toBe(60) // past the end → hold last
  })

  it('is blank (NaN) before the first sample and keeps NaN gaps visible', () => {
    expect(Number.isNaN(stepHoldAt(t, v, -0.5))).toBe(true)
    const vv = new Float64Array([10, NaN, 30, 40, 50, 60])
    expect(Number.isNaN(stepHoldAt(t, vv, 1.5))).toBe(true) // held sample is a gap
  })
})
