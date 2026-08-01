// Phase 10 frontend tests: the measurement client after the REST tier was
// deleted. Two things are worth asserting and nothing else really is —
//
//   1. the compatibility shim holds. `status` and the flat `payload` are read
//      by files this module does not own (channelStore branches on 404,
//      calc.ts reads payload.suggestedDt), so every typed engine code has to
//      land on the number the Spring controller used to send.
//   2. the file is *moved* into the worker, not copied. That one is checked
//      against the real engineClient with a fake Worker, because a mock of
//      engineClient cannot tell a transfer from a clone — and the difference
//      is the whole reason a 300 MB recording opens at all.
//
// Plus the negative that names the phase: no call touches fetch.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const engine = vi.hoisted(() => ({
  open: vi.fn(),
  channelWindow: vi.fn(),
  calc: vi.fn(),
  close: vi.fn(),
}))

vi.mock('../wasm/engineClient', () => ({
  wasmMeasurementOpen: engine.open,
  wasmMeasurementChannelWindow: engine.channelWindow,
  wasmMeasurementCalc: engine.calc,
  wasmMeasurementClose: engine.close,
}))

import {
  calcSignal,
  deleteMeasurement,
  fetchChannelWindow,
  MeasurementApiError,
  uploadMeasurement,
  type CalcRequestDto,
} from './measurementApi'

const METADATA = {
  ok: true as const,
  measurementId: 'm-1',
  name: 'run.mf4',
  size: 4,
  metadata: {
    groups: [
      {
        index: 0,
        name: 'g0',
        records: 3,
        channels: [
          { name: 't', unit: 's', timeMaster: true, kind: 'analog' },
          { name: 'speed', unit: 'm/s', timeMaster: false, kind: 'analog' },
        ],
      },
    ],
  },
}

const CALC_REQUEST: CalcRequestDto = {
  name: 'p',
  formula: 'x * 2',
  inputs: [{ var: 'x', interp: 'linear', measurementId: 'm-1', channel: 'speed', group: 0 }],
  raster: { mode: 'merge' },
}

/** The failure envelope the wasm boundary emits for a MeasurementError. */
function failure(code: string, message: string, extras: Record<string, unknown> = {}) {
  return { ok: false as const, error: { code, message, ...extras } }
}

let fetchSpy: ReturnType<typeof vi.fn>

beforeEach(() => {
  engine.open.mockReset()
  engine.channelWindow.mockReset()
  engine.calc.mockReset()
  engine.close.mockReset()
  engine.close.mockResolvedValue(undefined)
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

describe('uploadMeasurement', () => {
  it('reads the File and returns the engine metadata', async () => {
    engine.open.mockResolvedValue(METADATA)
    const file = new File([new Uint8Array([1, 2, 3, 4])], 'run.mf4')

    const result = await uploadMeasurement(file)

    expect(result.measurementId).toBe('m-1')
    expect(result.metadata.groups[0].channels[1].name).toBe('speed')
    const [bytes, name] = engine.open.mock.calls[0] as [Uint8Array, string]
    expect(name).toBe('run.mf4')
    expect(bytes).toBeInstanceOf(Uint8Array)
    expect([...bytes]).toEqual([1, 2, 3, 4])
    expect(fetchSpy).not.toHaveBeenCalled()
  })

  it('maps MEASUREMENT_PARSE_FAILED to 400', async () => {
    engine.open.mockResolvedValue(failure('MEASUREMENT_PARSE_FAILED', 'DZ compression is not supported.'))
    const file = new File([new Uint8Array([0])], 'bad.mf4')

    const err = await uploadMeasurement(file).catch((e: unknown) => e)

    expect(err).toBeInstanceOf(MeasurementApiError)
    const api = err as MeasurementApiError
    expect(api.name).toBe('MeasurementApiError')
    expect(api.status).toBe(400)
    expect(api.message).toBe('DZ compression is not supported.')
    expect(api.payload).toEqual({
      code: 'MEASUREMENT_PARSE_FAILED',
      error: 'DZ compression is not supported.',
    })
  })
})

describe('fetchChannelWindow', () => {
  it('sends the REST parameters as the request body and returns the window', async () => {
    engine.channelWindow.mockResolvedValue({
      ok: true,
      t: [0, 1, 2],
      v: [1, 2, 3],
      min: null,
      max: null,
      decimated: false,
      totalSamples: 3,
      unit: 'm/s',
      kind: 'analog',
    })

    const win = await fetchChannelWindow('m-1', 0, 'speed', 1.5, 9, 2000)

    expect(win.t).toEqual([0, 1, 2])
    expect(win.decimated).toBe(false)
    expect(JSON.parse(engine.channelWindow.mock.calls[0][0] as string)).toEqual({
      measurementId: 'm-1',
      group: 0,
      channel: 'speed',
      from: 1.5,
      to: 9,
      maxPoints: 2000,
    })
    expect(fetchSpy).not.toHaveBeenCalled()
  })

  it('keeps null bounds null rather than dropping them', async () => {
    engine.channelWindow.mockResolvedValue({
      ok: true,
      t: [],
      v: [],
      min: null,
      max: null,
      decimated: false,
      totalSamples: 0,
      unit: null,
      kind: 'analog',
    })

    await fetchChannelWindow('m-1', 0, 'speed', null, null, 500)

    const body = JSON.parse(engine.channelWindow.mock.calls[0][0] as string) as Record<string, unknown>
    expect(body.from).toBeNull()
    expect(body.to).toBeNull()
  })

  it('reads a null sample back as NaN, not as zero', async () => {
    // The boundary encodes a gap as null (JSON has no NaN). If it survived as
    // null, channelStore's `Float64Array.from` would turn it into 0 and draw a
    // spike to zero — a fabricated reading. Every sample array is covered.
    engine.channelWindow.mockResolvedValue({
      ok: true,
      t: [0, null, 2],
      v: [1, null, 3],
      min: [1, null, 3],
      max: [2, null, 4],
      decimated: true,
      totalSamples: 3,
      unit: 'm/s',
      kind: 'analog',
    })

    const win = await fetchChannelWindow('m-1', 0, 'speed', null, null, 500)

    expect(win.t[1]).toBeNaN()
    expect(win.v?.[1]).toBeNaN()
    expect(win.min?.[1]).toBeNaN()
    expect(win.max?.[1]).toBeNaN()
    // Finite neighbours are untouched — this converts holes, nothing else.
    expect(win.t[0]).toBe(0)
    expect(win.v?.[2]).toBe(3)
    // The point of doing it here: the consumer's own conversion now preserves
    // the gap instead of inventing a zero.
    expect(Float64Array.from(win.t)[1]).toBeNaN()
  })

  it('keeps an absent array null rather than turning it into an empty one', async () => {
    // A null *array* and a null *element* mean different things: `v` is absent
    // on a decimated window, min/max on an undecimated one. Only the second is
    // a gap, so the first must stay null or the UI loses the distinction.
    engine.channelWindow.mockResolvedValue({
      ok: true,
      t: [0, 1],
      v: [5, 6],
      min: null,
      max: null,
      decimated: false,
      totalSamples: 2,
      unit: null,
      kind: 'analog',
    })

    const win = await fetchChannelWindow('m-1', 0, 'speed', null, null, 500)

    expect(win.min).toBeNull()
    expect(win.max).toBeNull()
    expect(win.v).toEqual([5, 6])
  })

  it('maps CHANNEL_NOT_FOUND to 404 — the code channelStore branches on', async () => {
    engine.channelWindow.mockResolvedValue(failure('CHANNEL_NOT_FOUND', 'No channel "torque" in group 0.'))

    const err = (await fetchChannelWindow('m-1', 0, 'torque', null, null, 500).catch(
      (e: unknown) => e,
    )) as MeasurementApiError

    expect(err).toBeInstanceOf(MeasurementApiError)
    expect(err.status).toBe(404)
    expect(err.payload?.code).toBe('CHANNEL_NOT_FOUND')
  })

  it('reports a worker-level failure as a 500 MeasurementApiError', async () => {
    // Not an envelope — the engine client rejected, i.e. the worker itself died.
    engine.channelWindow.mockRejectedValue(new Error('The engine worker failed'))

    const err = (await fetchChannelWindow('m-1', 0, 'speed', null, null, 500).catch(
      (e: unknown) => e,
    )) as MeasurementApiError

    expect(err).toBeInstanceOf(MeasurementApiError)
    expect(err.status).toBe(500)
    expect(err.message).toBe('The engine worker failed')
    expect(err.payload).toBeNull()
  })
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

  it('reads a null in the result series back as NaN', async () => {
    // A formula evaluated over a gap yields a gap; it crosses the wire the
    // same way a recorded sample does and has to come back the same way.
    engine.calc.mockResolvedValue({ ok: true, name: 'p', t: [0, 1, 2], v: [2, null, 6] })

    const result = await calcSignal(CALC_REQUEST)

    expect(result.v[1]).toBeNaN()
    expect(result.v[0]).toBe(2)
    expect(result.t).toEqual([0, 1, 2])
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
})

describe('deleteMeasurement', () => {
  it('is fire-and-forget: returns void and swallows a rejection', async () => {
    engine.close.mockRejectedValue(new Error('worker already gone'))
    const unhandled = vi.fn()
    process.on('unhandledRejection', unhandled)

    expect(deleteMeasurement('m-1')).toBeUndefined()
    expect(engine.close).toHaveBeenCalledWith('m-1')
    await new Promise((resolve) => setTimeout(resolve, 0))

    process.off('unhandledRejection', unhandled)
    expect(unhandled).not.toHaveBeenCalled()
    expect(fetchSpy).not.toHaveBeenCalled()
  })
})

// ── the transfer, against the real client ───────────────────────────────────
//
// engineClient is mocked above, so these use vi.importActual and a fake Worker:
// the point is the second argument to postMessage, which a mock cannot show.

interface Posted {
  message: { id: number; bytes?: Uint8Array }
  transfer?: unknown[]
}

// Statics, not instance state: engineClient spawns *one* worker and keeps it,
// so every test in this block talks to the same fake and the log has to
// outlive each one. `latest()` is therefore always the same object after the
// first spawn — which is exactly the invariant the REPL depends on.
class FakeWorker {
  static instances: FakeWorker[] = []
  static posted: Posted[] = []
  onmessage: ((event: MessageEvent) => void) | null = null
  onerror: ((event: unknown) => void) | null = null
  onmessageerror: (() => void) | null = null
  constructor() {
    FakeWorker.instances.push(this)
  }
  postMessage(message: unknown, transfer?: unknown[]) {
    FakeWorker.posted.push({ message: message as Posted['message'], transfer })
  }
  terminate() {}
}

function latest(): { worker: FakeWorker; post: Posted } {
  const worker = FakeWorker.instances.at(-1)
  const post = FakeWorker.posted.at(-1)
  if (!worker || !post) throw new Error('the engine client posted nothing')
  return { worker, post }
}

/** Feeds back the worker envelope the real engine.worker.ts would have sent. */
function reply(result: string): void {
  const { worker, post } = latest()
  worker.onmessage?.({ data: { id: post.message.id, ok: true, result } } as MessageEvent)
}

describe('engineClient measurement transport', () => {
  beforeEach(() => {
    vi.stubGlobal('Worker', FakeWorker)
  })

  it('transfers the file buffer instead of cloning it', async () => {
    const client = await vi.importActual<typeof import('../wasm/engineClient')>(
      '../wasm/engineClient',
    )
    const bytes = new Uint8Array([9, 8, 7, 6])
    const buffer = bytes.buffer

    const pending = client.wasmMeasurementOpen<{ measurementId: string }>(bytes, 'run.mf4')

    const { post } = latest()
    expect(post.message.bytes).toBe(bytes)
    // The assertion that matters: the backing ArrayBuffer is in the transfer
    // list, so a 300 MB recording is moved, not memcpy'd into a second heap.
    expect(post.transfer).toEqual([buffer])

    reply('{"ok":true,"measurementId":"m-9"}')
    await expect(pending).resolves.toEqual({ ok: true, measurementId: 'm-9' })
  })

  it('sends no transfer list for the string-only methods', async () => {
    const client = await vi.importActual<typeof import('../wasm/engineClient')>(
      '../wasm/engineClient',
    )

    const pending = client.wasmMeasurementCalc<{ name: string }>('{"name":"p"}')

    const { post } = latest()
    expect(post.transfer).toBeUndefined()
    expect(post.message.bytes).toBeUndefined()

    reply('{"ok":true,"name":"p"}')
    await expect(pending).resolves.toEqual({ ok: true, name: 'p' })
  })

  it('turns a shapeless boundary response into a typed failure', async () => {
    const client = await vi.importActual<typeof import('../wasm/engineClient')>(
      '../wasm/engineClient',
    )

    const closing = client.wasmMeasurementClose('m-1')
    reply('""')
    await expect(closing).resolves.toBeUndefined()

    const pending = client.wasmMeasurementCalc<{ name: string }>('{}')
    reply('{"unexpected":1}')
    await expect(pending).resolves.toEqual({
      ok: false,
      error: {
        code: 'MEASUREMENT_PARSE_FAILED',
        message: 'The measurement boundary returned an unrecognised response.',
      },
    })
  })
})
