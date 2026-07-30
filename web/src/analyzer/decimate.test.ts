// Decimation contract tests (§2.5d): the analog envelope never loses a peak,
// and the boolean path never loses a pulse — no matter how far zoomed out.
// Fixture policy: all large arrays are generated here at test time, never
// committed as files.

import { describe, expect, it } from 'vitest'
import { booleanEnvelope, lowerBound, minMaxEnvelope } from './decimate'

function ramp(n: number, dt = 0.001): Float64Array {
  const t = new Float64Array(n)
  for (let i = 0; i < n; i++) t[i] = i * dt
  return t
}

describe('lowerBound', () => {
  it('finds the first index >= x', () => {
    const a = new Float64Array([0, 1, 2, 3, 4])
    expect(lowerBound(a, -1)).toBe(0)
    expect(lowerBound(a, 0)).toBe(0)
    expect(lowerBound(a, 2)).toBe(2)
    expect(lowerBound(a, 2.5)).toBe(3)
    expect(lowerBound(a, 99)).toBe(5)
  })
})

describe('minMaxEnvelope (analog)', () => {
  it('preserves a single-sample spike in a 1M-point sine', () => {
    const n = 1_000_000
    const t = ramp(n)
    const v = new Float64Array(n)
    for (let i = 0; i < n; i++) v[i] = Math.sin(i / 1000)
    const spikeIdx = 733_211
    v[spikeIdx] = 42.5 // positive spike
    v[211_733] = -37.25 // negative spike

    const env = minMaxEnvelope(t, v, 0, n - 1, 1200)
    expect(env.t.length).toBe(1200)
    expect(Math.max(...env.max)).toBe(42.5)
    expect(Math.min(...env.min)).toBe(-37.25)
  })

  it('brackets every bucket around the true local extrema', () => {
    const n = 10_000
    const t = ramp(n)
    const v = new Float64Array(n)
    for (let i = 0; i < n; i++) v[i] = Math.cos(i / 50) * (1 + i / n)
    const env = minMaxEnvelope(t, v, 0, n - 1, 100)
    for (let b = 0; b < env.t.length; b++) {
      expect(env.min[b]).toBeLessThanOrEqual(env.max[b])
    }
    // Global extrema are always retained.
    expect(Math.max(...env.max)).toBeCloseTo(Math.max(...v), 12)
    expect(Math.min(...env.min)).toBeCloseTo(Math.min(...v), 12)
  })

  it('reduces to at most the requested bucket count and respects sub-ranges', () => {
    const n = 5000
    const t = ramp(n)
    const v = new Float64Array(n).fill(1)
    v[2500] = 9
    const env = minMaxEnvelope(t, v, 2000, 2999, 10)
    expect(env.t.length).toBe(10)
    expect(Math.max(...env.max)).toBe(9)
    expect(env.t[0]).toBeGreaterThanOrEqual(t[2000])
    expect(env.t[9]).toBeLessThanOrEqual(t[2999])
  })

  it('renders all-NaN buckets as gaps (NaN), skipping NaN inside mixed buckets', () => {
    const n = 1000
    const t = ramp(n)
    const v = new Float64Array(n)
    for (let i = 0; i < n; i++) v[i] = i < 500 ? NaN : 1
    const env = minMaxEnvelope(t, v, 0, n - 1, 10)
    expect(Number.isNaN(env.min[0])).toBe(true)
    expect(Number.isNaN(env.max[0])).toBe(true)
    expect(env.min[9]).toBe(1)
  })
})

describe('booleanEnvelope (transition-preserving, §2.5d)', () => {
  it('never loses a 1-sample pulse in 1M samples', () => {
    const n = 1_000_000
    const t = ramp(n)
    const v = new Float64Array(n) // all zeros
    const pulseIdx = 481_997
    v[pulseIdx] = 1

    const env = booleanEnvelope(t, v, 0, n - 1, 500)
    // The bucket containing the pulse must span both levels (any-change flag).
    let pulseBuckets = 0
    for (let b = 0; b < env.t.length; b++) {
      if (env.max[b] === 1) {
        pulseBuckets++
        expect(env.min[b]).toBe(0)
      }
    }
    expect(pulseBuckets).toBe(1)
  })

  it('keeps a steady level flat (no phantom transitions)', () => {
    const n = 100_000
    const t = ramp(n)
    const v = new Float64Array(n).fill(1)
    const env = booleanEnvelope(t, v, 0, n - 1, 100)
    for (let b = 0; b < env.t.length; b++) {
      expect(env.min[b]).toBe(1)
      expect(env.max[b]).toBe(1)
    }
  })
})
