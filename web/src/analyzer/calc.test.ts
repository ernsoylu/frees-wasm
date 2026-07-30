// Phase 4 frontend tests: calc request DTO mapping and the over-cap
// recovery path (typed RASTER_CAP_EXCEEDED → one-click fixed-dt switch).

import { describe, expect, it } from 'vitest'
import {
  applyOverCap,
  buildCalcRequest,
  calcResultToMeasurement,
  parseOverCap,
  sniffKind,
} from './calc'
import { channelStore } from './channelStore'
import { MeasurementApiError } from './measurementApi'
import type { ImportedMeasurement } from './csvImport'

function fakeMeasurement(name: string, channel: string): ImportedMeasurement {
  return {
    signatureName: name,
    size: 100,
    headerHash: 'x',
    time: Float64Array.from([0, 1, 2]),
    rowCount: 3,
    channels: [
      { name: channel, unit: 'm/s', kind: 'analog', values: Float64Array.from([1, 2, 3]), min: 1, max: 3 },
    ],
  }
}

describe('buildCalcRequest', () => {
  it('maps local signals to inline series', () => {
    const meta = channelStore.register(fakeMeasurement('run.csv', 'speed'), 'calc-test-analyzer')
    const request = buildCalcRequest(
      'p',
      'x * 2',
      [{ var: 'x', signal: { measurementId: meta.measurementId, channel: 'speed' }, interp: 'linear' }],
      { mode: 'merge' },
    )
    expect(request.inputs).toHaveLength(1)
    expect(request.inputs[0].inline?.t).toEqual([0, 1, 2])
    expect(request.inputs[0].inline?.v).toEqual([1, 2, 3])
    expect(request.inputs[0].measurementId).toBeUndefined()
    channelStore.release(meta.measurementId, 'calc-test-analyzer')
  })

  it('throws a user-facing error for unknown measurements', () => {
    expect(() =>
      buildCalcRequest(
        'p',
        'x',
        [{ var: 'x', signal: { measurementId: 'nope', channel: 'gone' }, interp: 'step' }],
        { mode: 'merge' },
      ),
    ).toThrow(/not in memory/)
  })
})

describe('over-cap recovery (guided path)', () => {
  it('parses the typed payload and produces the fixed-dt raster', () => {
    const err = new MeasurementApiError(422, 'over cap', {
      code: 'RASTER_CAP_EXCEEDED',
      actualPoints: 2_000_000,
      suggestedDt: 0.002,
    })
    const info = parseOverCap(err)
    expect(info).toEqual({ actualPoints: 2_000_000, suggestedDt: 0.002 })
    expect(applyOverCap(info as NonNullable<typeof info>)).toEqual({ mode: 'fixed', dt: 0.002 })
  })

  it('ignores unrelated errors', () => {
    expect(parseOverCap(new Error('boom'))).toBeNull()
    expect(parseOverCap(new MeasurementApiError(422, 'other', { code: 'OTHER' }))).toBeNull()
  })
})

describe('calc result → measurement', () => {
  it('wraps the series with kind sniffing and extents', () => {
    const m = calcResultToMeasurement('valve_ok', [0, 1, 2], [0, 1, 0])
    expect(m.signatureName).toBe('ƒ valve_ok')
    expect(m.channels[0].kind).toBe('boolean')
    expect(m.channels[0].min).toBe(0)
    expect(m.channels[0].max).toBe(1)
    expect(sniffKind(Float64Array.from([0, 2, 1]))).toBe('analog')
  })
})
