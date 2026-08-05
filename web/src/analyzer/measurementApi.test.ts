// Tests for the measurement client after the MDF4 removal (decision D6):
// exactly one call remains — calculated signals. Two things are worth
// asserting and nothing else really is —
//
//   1. the compatibility shim holds. `status` and the flat `payload` are read
//      by files this module does not own (calc.ts reads payload.suggestedDt),
//      so every typed engine code has to land on the number the Spring
//      controller used to send.
//   2. the boundary's null-for-NaN encoding is undone here, so a gap in a
//      result series never becomes a fabricated zero downstream.
//
// Plus the negative that names the phase: no call touches fetch.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const engine = vi.hoisted(() => ({
  calc: vi.fn(),
}))

vi.mock('../wasm/engineClient', () => ({
  wasmMeasurementCalc: engine.calc,
}))

import { calcSignal, MeasurementApiError, type CalcRequestDto } from './measurementApi'

const CALC_REQUEST: CalcRequestDto = {
  name: 'p',
  formula: 'x * 2',
  inputs: [{ var: 'x', interp: 'linear', inline: { t: [0, 1], v: [1, 2] } }],
  raster: { mode: 'merge' },
}

/** The failure envelope the wasm boundary emits for a MeasurementError. */
function failure(code: string, message: string, extras: Record<string, unknown> = {}) {
  return { ok: false as const, error: { code, message, ...extras } }
}

let fetchSpy: ReturnType<typeof vi.fn>

beforeEach(() => {
  engine.calc.mockReset()
  // Any /api/ traffic at all is a regression, so make it loud rather than
  // letting jsdom's own fetch quietly attempt a request.
  fetchSpy = vi.fn(() => {
    throw new Error('measurementApi must not call fetch')
  })
  vi.stubGlobal('fetch', fetchSpy)
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('calcSignal', () => {
  it('resolves in a single engine call — the 202 + poll path is gone', async () => {
    engine.calc.mockResolvedValue({ ok: true, name: 'p', t: [0, 1], v: [2, 4] })

    const result = await calcSignal(CALC_REQUEST)

    expect(result).toEqual({ ok: true, name: 'p', t: [0, 1], v: [2, 4] })
    expect(engine.calc).toHaveBeenCalledTimes(1)
    expect(JSON.parse(engine.calc.mock.calls[0][0] as string)).toEqual(CALC_REQUEST)
    expect(fetchSpy).not.toHaveBeenCalled()
  })

  it('reads a null in the result series back as NaN, not as zero', async () => {
    // A formula evaluated over a gap yields a gap; JSON has no NaN, so the
    // boundary writes it as null. If it survived as null, channelStore's
    // `Float64Array.from` would turn it into 0 and draw a spike to zero — a
    // fabricated reading. Both sample arrays are covered.
    engine.calc.mockResolvedValue({ ok: true, name: 'p', t: [0, null, 2], v: [2, null, 6] })

    const result = await calcSignal(CALC_REQUEST)

    expect(result.t[1]).toBeNaN()
    expect(result.v[1]).toBeNaN()
    // Finite neighbours are untouched — this converts holes, nothing else.
    expect(result.t[0]).toBe(0)
    expect(result.v[2]).toBe(6)
    // The point of doing it here: the consumer's own conversion now preserves
    // the gap instead of inventing a zero.
    expect(Float64Array.from(result.v)[1]).toBeNaN()
  })

  it('maps MEASUREMENT_PARSE_FAILED to 400', async () => {
    engine.calc.mockResolvedValue(failure('MEASUREMENT_PARSE_FAILED', 'The request is unusable.'))

    const err = (await calcSignal(CALC_REQUEST).catch((e: unknown) => e)) as MeasurementApiError

    expect(err).toBeInstanceOf(MeasurementApiError)
    expect(err.name).toBe('MeasurementApiError')
    expect(err.status).toBe(400)
    expect(err.message).toBe('The request is unusable.')
    expect(err.payload).toEqual({
      code: 'MEASUREMENT_PARSE_FAILED',
      error: 'The request is unusable.',
    })
  })

  it('maps FORMULA_ERROR to 422', async () => {
    engine.calc.mockResolvedValue(failure('FORMULA_ERROR', 'Unbound input "y".'))

    const err = (await calcSignal(CALC_REQUEST).catch((e: unknown) => e)) as MeasurementApiError

    expect(err.status).toBe(422)
    expect(err.payload?.code).toBe('FORMULA_ERROR')
  })

  it('maps RASTER_CAP_EXCEEDED to 422 and flattens the recovery payload', async () => {
    engine.calc.mockResolvedValue(
      failure('RASTER_CAP_EXCEEDED', 'The merged raster has 2000000 points…', {
        actualPoints: 2_000_000,
        suggestedDt: 0.002,
        cap: 500_000,
      }),
    )

    const err = (await calcSignal(CALC_REQUEST).catch((e: unknown) => e)) as MeasurementApiError

    expect(err.status).toBe(422)
    // calc.ts::parseOverCap reads exactly these three off `payload`, flat.
    expect(err.payload?.code).toBe('RASTER_CAP_EXCEEDED')
    expect(err.payload?.actualPoints).toBe(2_000_000)
    expect(err.payload?.suggestedDt).toBe(0.002)
  })

  it('falls back to 500 for a code this build does not know', async () => {
    engine.calc.mockResolvedValue(failure('SOME_FUTURE_CODE', 'Nope.'))

    const err = (await calcSignal(CALC_REQUEST).catch((e: unknown) => e)) as MeasurementApiError

    // 500 because no caller special-cases it: an unknown variant degrades to
    // "show the message", never to a wrong recovery.
    expect(err.status).toBe(500)
    expect(err.payload?.code).toBe('SOME_FUTURE_CODE')
  })

  it('reports a worker-level failure as a 500 MeasurementApiError', async () => {
    // Not an envelope — the engine client rejected, i.e. the worker itself died.
    engine.calc.mockRejectedValue(new Error('The engine worker failed'))

    const err = (await calcSignal(CALC_REQUEST).catch((e: unknown) => e)) as MeasurementApiError

    expect(err).toBeInstanceOf(MeasurementApiError)
    expect(err.status).toBe(500)
    expect(err.message).toBe('The engine worker failed')
    expect(err.payload).toBeNull()
  })
})
