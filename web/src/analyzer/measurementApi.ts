// Client for the measurement engine, which now runs *in this tab*.
//
// The Java tier read .mf4 files server-side because that is where the parser
// lived: the file was uploaded to /api/measurements, indexed on disk under a
// TTL, and read back as windowed decimated envelopes. The comment that used to
// sit here said "the browser never holds the raw file", and treated that as the
// feature. With the parser compiled to wasm the sentence inverts, and so does
// the argument: the browser holds the raw file and **nothing else ever does**.
//
// That is the better arrangement, and not only because it deletes a round trip.
// Measurement recordings are test-bench data — vehicle logs, rig runs, customer
// acceptance traces — and are routinely confidential in a way an equation
// document is not. Uploading one bought nothing except a parser; now that the
// parser is here, the upload was pure exposure. A .mf4 opened in this app never
// leaves the machine, is never written to a server disk, and needs no TTL sweep
// to prove it was forgotten.
//
// What survives from the REST era is the *shape*: the exported functions, their
// signatures, the Remote*/Calc* DTOs, and MeasurementApiError with its `status`
// and `payload`. Every other file in analyzer/ imports from here and compiles
// unchanged. Zero /api/ traffic.

import {
  wasmMeasurementCalc,
  wasmMeasurementChannelWindow,
  wasmMeasurementClose,
  wasmMeasurementOpen,
  type MeasurementEnvelope,
  type MeasurementErrorBody,
} from '../wasm/engineClient'

export interface RemoteChannelInfo {
  name: string
  unit?: string | null
  timeMaster: boolean
  kind: 'analog' | 'boolean' | 'string'
}

export interface RemoteGroupInfo {
  index: number
  name: string
  records: number
  channels: RemoteChannelInfo[]
}

export interface RemoteMeasurement {
  measurementId: string
  name: string
  size: number
  metadata: { groups: RemoteGroupInfo[] }
}

export interface RemoteWindow {
  t: number[]
  v: number[] | null
  min: number[] | null
  max: number[] | null
  decimated: boolean
  totalSamples: number
  unit?: string | null
  kind: string
}

/** Error carrying the HTTP status + typed payload (e.g. RASTER_CAP_EXCEEDED). */
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
 * These numbers are a **compatibility shim, not HTTP**. Nothing here speaks
 * HTTP any more; the engine returns `MeasurementError::code()` and a message.
 * But `status` is load-bearing in code this module does not own —
 * channelStore.ts treats 404 as "this measurement is gone, mark the entry
 * evicted and degrade to a re-import placeholder" — so the mapping has to keep
 * meaning what it meant:
 *
 *   CHANNEL_NOT_FOUND       404  the id/group/channel is not there. The only
 *                                code channelStore branches on.
 *   RASTER_CAP_EXCEEDED     422  a well-formed request the engine refuses;
 *   FORMULA_ERROR           422  the user must change something and retry.
 *   MEASUREMENT_PARSE_FAILED 400 the *input itself* is unusable — an .mf4 this
 *                                reader cannot decode. Distinct from 422
 *                                because no edit to the request fixes it; the
 *                                recording has to be re-exported.
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
 * callers already parse. The Spring controller sent a flat body — `{error,
 * code, actualPoints, suggestedDt}` — and calc.ts::parseOverCap reads
 * `payload.code` / `payload.actualPoints` / `payload.suggestedDt` straight off
 * it, so the nesting the wasm boundary uses is undone here rather than there.
 */
function toApiError(error: MeasurementErrorBody): MeasurementApiError {
  const { code, message, ...extras } = error
  const text = typeof message === 'string' ? message : 'The measurement engine reported a failure.'
  const status = (typeof code === 'string' && STATUS_BY_CODE[code]) || 500
  return new MeasurementApiError(status, text, { ...extras, code, error: text })
}

/**
 * The window exactly as the boundary writes it — gaps are still `null`.
 *
 * Kept separate from `RemoteWindow` so the difference between the wire and the
 * app type is stated rather than assumed: everything below converts one into
 * the other, and `RemoteWindow`'s `number[]` is only true after it has.
 */
type WireWindow = Omit<RemoteWindow, 't' | 'v' | 'min' | 'max'> & {
  t: (number | null)[]
  v: (number | null)[] | null
  min: (number | null)[] | null
  max: (number | null)[] | null
}

/** Ditto for a calculated signal: a formula over a gap yields a gap. */
type WireCalcResult = Omit<CalcResultDto, 't' | 'v'> & {
  t: (number | null)[]
  v: (number | null)[]
}

/**
 * Undoes the boundary's non-finite encoding: `null` back to `NaN`.
 *
 * JSON has no `NaN`, and a gap in measured data *is* `NaN` — never an absent
 * sample — so `finite_or_null` in crates/frees-wasm/src/measurement.rs writes
 * holes as `null` rather than fabricating a `0`.
 *
 * The hole does not survive the trip on its own, and the failure is silent:
 * `Float64Array.from([1, null])` is `[1, 0]`, and channelStore.ts builds its
 * columns exactly that way, so an unconverted `null` draws a gap as a **spike
 * to zero** — the invented data point the encoding exists to prevent. Undoing
 * it here, at the module that owns the wire types, rather than at each consumer
 * means `RemoteWindow.t: number[]` is honest: by the time a window leaves this
 * function there are no `null`s left in it.
 *
 * A `null` *array* is a different thing from a `null` element — `v` is absent
 * on a decimated window, `min`/`max` on an undecimated one — so it stays null.
 */
function nanFilled(values: (number | null)[] | null): number[] | null {
  if (values === null) return null
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

/**
 * Hands a .mf4 to the engine and gets its metadata back.
 *
 * Still takes a `File` — SignalBrowser passes one straight from the file
 * picker. `arrayBuffer()` gives a fresh buffer the engine client then
 * *transfers* into the worker rather than copying; see engineClient.call for
 * why that distinction decides whether a large recording opens at all.
 */
export async function uploadMeasurement(file: File): Promise<RemoteMeasurement> {
  const bytes = new Uint8Array(await file.arrayBuffer())
  return unwrap(wasmMeasurementOpen<RemoteMeasurement>(bytes, file.name))
}

export async function fetchChannelWindow(
  id: string,
  group: number,
  channel: string,
  from: number | null,
  to: number | null,
  maxPoints: number,
): Promise<RemoteWindow> {
  // The old query string, transcribed: the field names are the REST parameter
  // names so the boundary and the route stay legible against each other.
  const request = JSON.stringify({ measurementId: id, group, channel, from, to, maxPoints })
  const window = await unwrap(wasmMeasurementChannelWindow<WireWindow>(request))
  return {
    ...window,
    t: nanFilled(window.t) ?? [],
    v: nanFilled(window.v),
    min: nanFilled(window.min),
    max: nanFilled(window.max),
  }
}

export interface CalcRequestDto {
  name: string
  formula: string
  inputs: {
    var: string
    interp: 'step' | 'linear'
    measurementId?: string
    channel?: string
    group?: number
    inline?: { t: number[]; v: number[] }
  }[]
  raster: { mode: 'merge' } | { mode: 'fixed'; dt: number } | { mode: 'sameAs'; sameAs: string }
}

export interface CalcResultDto {
  name: string
  t: number[]
  v: number[]
}

/**
 * Evaluate a calculated signal (Phase 4).
 *
 * The REST version had two paths: small formulas answered synchronously, and
 * call-bearing formulas over large rasters came back 202 + jobId to be polled
 * against the Redis job store. Both the broker and the store existed only
 * because compute was remote, so **the polling is gone** — there is no job to
 * poll, and the worker round-trip already keeps the UI thread free.
 *
 * The signature does not change, and deliberately: `async` here is the seam
 * CLAUDE.md tells the port to preserve, so CalcSignalModal's `await
 * calcSignal(request)` needs no rewrite and the 202 path can never be missed by
 * a caller that forgot it existed.
 */
export async function calcSignal(request: CalcRequestDto): Promise<CalcResultDto> {
  const result = await unwrap(wasmMeasurementCalc<WireCalcResult>(JSON.stringify(request)))
  return { ...result, t: nanFilled(result.t) ?? [], v: nanFilled(result.v) ?? [] }
}

export function deleteMeasurement(id: string): void {
  // Still fire-and-forget, but the safety net changed: there is no TTL sweep
  // any more, only the tab closing. Failing to free is a leak inside this page,
  // not an orphan on someone's disk.
  void wasmMeasurementClose(id).catch(() => undefined)
}
