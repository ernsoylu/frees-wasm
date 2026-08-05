// Client for the measurement engine, which runs *in this tab*.
//
// After the MDF4 removal (decision D6) exactly one call remains: calculated
// signals. Measurement data enters the app as CSV parsed on the main thread
// into channelStore's Float64Array columns; a calculated signal samples those
// columns, ships them **inline** to the engine worker, and gets a column
// back. Nothing is uploaded, nothing is held in the engine, and there is no
// server — the same privacy story Phase 10 established, now with a smaller
// surface.
//
// What survives from the REST era is the *shape*: `calcSignal`'s signature,
// the Calc* DTOs, and MeasurementApiError with its `status` and `payload`.
// The `status` numbers are a compatibility shim, not HTTP — calc.ts's
// over-cap recovery branches on them.

import {
  wasmMeasurementCalc,
  type MeasurementEnvelope,
  type MeasurementErrorBody,
} from '../wasm/engineClient'

/** Error carrying the HTTP-era status + typed payload (e.g. RASTER_CAP_EXCEEDED). */
export class MeasurementApiError extends Error {
  readonly status: number
  readonly payload: Record<string, unknown> | null
  constructor(status: number, message: string, payload: Record<string, unknown> | null = null) {
    super(message)
    this.name = 'MeasurementApiError'
    this.status = status
    this.payload = payload
  }
}

/**
 * Typed error code → the status the REST tier used to answer with.
 *
 *   RASTER_CAP_EXCEEDED     422  a well-formed request the engine refuses;
 *   FORMULA_ERROR           422  the user must change something and retry.
 *   MEASUREMENT_PARSE_FAILED 400 the request itself is unusable.
 *   CHANNEL_NOT_FOUND       404  kept for wire compatibility; with every
 *                                input inline it should no longer occur.
 *
 * An unrecognised code gets 500, which no branch special-cases, so a future
 * variant degrades to "show the message" rather than to a wrong recovery.
 */
const STATUS_BY_CODE: Record<string, number> = {
  MEASUREMENT_PARSE_FAILED: 400,
  CHANNEL_NOT_FOUND: 404,
  FORMULA_ERROR: 422,
  RASTER_CAP_EXCEEDED: 422,
}

/**
 * Flattens the boundary's `{code, message, ...extras}` into the payload shape
 * callers already parse (calc.ts::parseOverCap reads `payload.code` /
 * `payload.actualPoints` / `payload.suggestedDt` straight off it).
 */
function toApiError(error: MeasurementErrorBody): MeasurementApiError {
  const { code, message, ...extras } = error
  const text = typeof message === 'string' ? message : 'The measurement engine reported a failure.'
  const status = (typeof code === 'string' && STATUS_BY_CODE[code]) || 500
  return new MeasurementApiError(status, text, { ...extras, code, error: text })
}

/** A calculated signal exactly as the boundary writes it — gaps are `null`. */
type WireCalcResult = Omit<CalcResultDto, 't' | 'v'> & {
  t: (number | null)[]
  v: (number | null)[]
}

/**
 * Undoes the boundary's non-finite encoding: `null` back to `NaN`.
 *
 * JSON has no `NaN`, and a gap in measured data *is* `NaN` — never an absent
 * sample — so the boundary writes holes as `null` rather than fabricating a
 * `0`. `Float64Array.from([1, null])` is `[1, 0]`, and channelStore builds
 * its columns exactly that way, so an unconverted `null` would draw a gap as
 * a **spike to zero** — the invented data point the encoding exists to
 * prevent.
 */
function nanFilled(values: (number | null)[]): number[] {
  return values.map((value) => (value === null ? NaN : value))
}

/** Resolves a boundary call to its payload, or throws MeasurementApiError. */
async function unwrap<T>(pending: Promise<MeasurementEnvelope<T>>): Promise<T> {
  let envelope: MeasurementEnvelope<T>
  try {
    envelope = await pending
  } catch (e) {
    // A rejection here is the *worker* failing — script load, an unexpected
    // wasm trap, an OOM. Call sites only recognise MeasurementApiError, so it
    // arrives as one, at a status no typed code maps to.
    throw new MeasurementApiError(500, e instanceof Error ? e.message : String(e))
  }
  if (!envelope.ok) throw toApiError(envelope.error)
  return envelope
}

export interface CalcRequestDto {
  name: string
  formula: string
  /** Every input series rides inline — the engine holds no recordings. */
  inputs: {
    var: string
    interp: 'step' | 'linear'
    inline: { t: number[]; v: number[] }
  }[]
  raster: { mode: 'merge' } | { mode: 'fixed'; dt: number } | { mode: 'sameAs'; sameAs: string }
}

export interface CalcResultDto {
  name: string
  t: number[]
  v: number[]
}

/**
 * Evaluate a calculated signal. Synchronous round-trip through the engine
 * worker; the REST version's 202+poll path died with the broker, and the
 * `async` signature is the seam that survives it.
 */
export async function calcSignal(request: CalcRequestDto): Promise<CalcResultDto> {
  const result = await unwrap(wasmMeasurementCalc<WireCalcResult>(JSON.stringify(request)))
  return { ...result, t: nanFilled(result.t), v: nanFilled(result.v) }
}
