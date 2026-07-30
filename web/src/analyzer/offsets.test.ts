// Phase 5a tests: per-file offset application and event-edge detection over
// offset display time.

import { describe, expect, it } from 'vitest'
import { channelStore } from './channelStore'
import { offsetExactValueAt, offsetRawRange, offsetWindow, offsetsOf } from './offsets'
import type { ImportedMeasurement } from './csvImport'
import type { AnalyzerSpec } from './types'

function measurement(): ImportedMeasurement {
  return {
    signatureName: 'r.csv',
    size: 10,
    headerHash: 'h',
    time: Float64Array.from([0, 1, 2, 3]),
    rowCount: 4,
    channels: [
      { name: 'v', unit: 'V', kind: 'analog', values: Float64Array.from([10, 20, 30, 40]), min: 10, max: 40 },
    ],
  }
}

describe('offset application', () => {
  it('shifts windows, raw ranges and exact lookups into display time', () => {
    const meta = channelStore.register(measurement(), 'offset-test')
    const ref = { measurementId: meta.measurementId, channel: 'v' }

    const win = offsetWindow(ref, 5, null, null, 100)
    expect(win?.t[0]).toBe(5)
    expect(win?.t[3]).toBe(8)
    expect(win?.v?.[0]).toBe(10)

    // A display-time window [6, 7] must map back to recorded [1, 2].
    const raw = offsetRawRange(ref, 5, 6, 7)
    expect(Array.from(raw?.t ?? [])).toEqual([6, 7])
    expect(Array.from(raw?.v ?? [])).toEqual([20, 30])

    const hit = offsetExactValueAt(ref, 5, 7.4)
    expect(hit?.t).toBe(7)
    expect(hit?.v).toBe(30)

    channelStore.release(meta.measurementId, 'offset-test')
  })

  it('offsetsOf reads only non-zero offsets from the spec', () => {
    const spec = {
      id: 'a',
      name: 'A',
      strips: [],
      files: [
        { measurementId: 'm1', signature: { name: 'x', size: 1, headerHash: 'h' }, offset: 2.5 },
        { measurementId: 'm2', signature: { name: 'y', size: 1, headerHash: 'h' } },
      ],
    } satisfies AnalyzerSpec
    const map = offsetsOf(spec)
    expect(map.get('m1')).toBe(2.5)
    expect(map.has('m2')).toBe(false)
  })
})

describe('event-edge detection (rising transitions)', () => {
  // The instrument's core loop, replicated over store data: cond false→true.
  it('finds rising edges once per pulse', () => {
    const meta = channelStore.register(
      {
        signatureName: 'p.csv',
        size: 10,
        headerHash: 'h',
        time: Float64Array.from([0, 1, 2, 3, 4, 5, 6]),
        rowCount: 7,
        channels: [
          {
            name: 'b',
            unit: undefined,
            kind: 'boolean',
            values: Float64Array.from([0, 1, 1, 0, 0, 1, 0]),
            min: 0,
            max: 1,
          },
        ],
      },
      'event-test',
    )
    const raw = offsetRawRange({ measurementId: meta.measurementId, channel: 'b' }, 10, null, null)
    const edges: number[] = []
    let prev = false
    for (let i = 0; i < (raw?.t.length ?? 0); i++) {
      const now = (raw?.v[i] ?? 0) > 0.5
      if (now && !prev) edges.push(raw?.t[i] ?? NaN)
      prev = now
    }
    expect(edges).toEqual([11, 15]) // offset by +10
    channelStore.release(meta.measurementId, 'event-test')
  })
})
