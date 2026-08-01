// Singleton client for the frees WASM engine worker.
//
// Lazily spawns engine.worker.ts on first use, correlates request/response
// pairs by id, and exposes typed async calls that JSON.parse the worker's
// result strings into the REST wire shapes api.ts already declares. A worker
// that dies (script load failure, OOM trap) rejects everything in flight and
// is dropped, so the next call spawns a fresh one instead of hanging forever.

import type {
  CheckResponse,
  DiagramResponse,
  LanguageReference,
  PsychartResponse,
  ReplResponse,
  SolveResponse,
} from '../api'
import type { EngineRequest, EngineResponse } from './engine.worker'

interface Pending {
  resolve: (result: string) => void
  reject: (reason: Error) => void
}

let worker: Worker | null = null
let nextId = 0
const pending = new Map<number, Pending>()

/** Rejects everything in flight and drops the worker so the next call respawns. */
function fail(reason: Error): void {
  const inFlight = [...pending.values()]
  pending.clear()
  worker?.terminate()
  worker = null
  for (const entry of inFlight) entry.reject(reason)
}

function spawn(): Worker {
  const w = new Worker(new URL('./engine.worker.ts', import.meta.url), {
    type: 'module',
  })
  w.onmessage = (event: MessageEvent<EngineResponse>) => {
    const response = event.data
    const entry = pending.get(response.id)
    if (!entry) return
    pending.delete(response.id)
    if (response.ok) {
      entry.resolve(response.result)
    } else {
      entry.reject(new Error(response.error))
    }
  }
  // A fired error event means the worker script itself failed (load/compile);
  // per-request problems arrive as {ok: false} messages instead.
  w.onerror = (event: ErrorEvent) => {
    fail(new Error(event.message || 'The engine worker failed'))
  }
  w.onmessageerror = () => {
    fail(new Error('The engine worker sent an unreadable message'))
  }
  return w
}

function call(method: EngineRequest['method'], args: string[]): Promise<string> {
  worker ??= spawn()
  const id = nextId++
  return new Promise<string>((resolve, reject) => {
    pending.set(id, { resolve, reject })
    worker?.postMessage({ id, method, args } satisfies EngineRequest)
  })
}

/** Runs a solve in the engine worker; resolves to the parsed SolveResponse. */
export async function wasmSolve(
  source: string,
  requestJson: string,
): Promise<SolveResponse> {
  return JSON.parse(await call('solve', [source, requestJson])) as SolveResponse
}

/** Runs a check in the engine worker; resolves to the parsed CheckResponse. */
export async function wasmCheck(
  source: string,
  requestJson: string,
): Promise<CheckResponse> {
  return JSON.parse(await call('check', [source, requestJson])) as CheckResponse
}

/** `POST /api/repl/evaluate`. The workspace lives inside the engine module —
 *  the last successful `wasmSolve` stored it — so this must go through the
 *  same worker, which it does (engineClient keeps exactly one). */
export async function wasmReplEvaluate(
  expression: string,
  unitSystem: string,
): Promise<ReplResponse> {
  return JSON.parse(
    await call('replEvaluate', [JSON.stringify({ expression, unitSystem })]),
  ) as ReplResponse
}

/** `POST /api/repl/clear`. `undefined` clears every REPL overlay; a name
 *  clears just that one. Resolves once the worker has done it. */
export async function wasmReplClear(name?: string): Promise<void> {
  await call('replClear', [JSON.stringify(name ?? null)])
}

/** The engine's language reference (units, built-in constants, intrinsics),
 *  read straight off the registries the solver itself uses. Argument-free, so
 *  the worker call carries no args. */
export async function wasmReference(): Promise<LanguageReference> {
  return JSON.parse(await call('reference', [])) as LanguageReference
}

/** The engine crate's semver, for the About dialog / worker handshake. */
export function wasmVersion(): Promise<string> {
  return call('version', [])
}

/** `GET /api/plot/fluids`. `available` is false and the list empty when the
 *  engine has no real-fluid property backend — the Java controller's own
 *  `CoolProp.isAvailable() ? plotFluids() : List.of()` branch. */
export async function wasmFluids(): Promise<{
  available: boolean
  fluids: string[]
  backend: string
}> {
  return JSON.parse(await call('fluids', [])) as {
    available: boolean
    fluids: string[]
    backend: string
  }
}

/** The wasm plot endpoints return `{error}` for a failure rather than throwing
 *  (the boundary's rule: document problems are data). The plot call sites want
 *  a rejected promise, so the error body becomes one here. */
function unwrapPlot<T>(payload: string): T {
  const parsed = JSON.parse(payload) as T & { error?: string }
  if (typeof parsed.error === 'string') throw new Error(parsed.error)
  return parsed
}

/** `POST /api/plot/propplot` — saturation dome, isolines and markers. */
export async function wasmPropertyDiagram(
  fluid: string,
  kind: string,
): Promise<DiagramResponse> {
  return unwrapPlot<DiagramResponse>(
    await call('propertyDiagram', [fluid, kind]),
  )
}

/** `POST /api/plot/psychart` — the psychrometric chart. */
export async function wasmPsychrometricChart(
  pressure: number,
  tMin: number,
  tMax: number,
): Promise<PsychartResponse> {
  return unwrapPlot<PsychartResponse>(
    await call('psychrometricChart', [JSON.stringify({ pressure, tMin, tMax })]),
  )
}
