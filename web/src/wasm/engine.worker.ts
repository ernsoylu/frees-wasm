// Module worker hosting the frees WASM engine off the UI thread.
//
// Protocol (see engineClient.ts, the only sender):
//   request  {id, method: 'solve' | 'check' | 'reference' | 'version' |
//                     'fluids' | 'propertyDiagram' | 'psychrometricChart' |
//                     'measurementOpen' | …,
//             args: string[], bytes?: Uint8Array}
//   response {id, ok: true, result: string} | {id, ok: false, error: string}
//
// `result` is the raw JSON string the wasm boundary emits (a REST-shaped
// SolveResponse/CheckResponse/LanguageReference; a bare semver string for
// 'version') — parsing happens on the client side, so the worker only ever
// posts strings.
//
// `bytes` is the one non-string channel, added for `measurementOpen`: a .mf4
// recording is the only input the engine takes that is megabytes of binary
// rather than a document. It rides as its own field so the client can hand the
// backing ArrayBuffer to postMessage's transfer list — see engineClient.call.
//
// Failure discipline: nothing may kill the worker. The wasm boundary already
// returns every *document* problem as data; this dispatch wraps the rest
// (init failure, unknown method, an unexpected trap) in {ok: false}.

import init, {
  check,
  fluids,
  measurement_calc,
  measurement_channel_window,
  measurement_close,
  measurement_open,
  property_diagram,
  psychrometric_chart,
  reference,
  repl_clear,
  repl_evaluate,
  solve,
  version,
} from './pkg/frees_wasm.js'

export interface EngineRequest {
  id: number
  method:
    | 'solve'
    | 'check'
    | 'reference'
    | 'version'
    | 'fluids'
    | 'propertyDiagram'
    | 'psychrometricChart'
    | 'replEvaluate'
    | 'replClear'
    | 'measurementOpen'
    | 'measurementChannelWindow'
    | 'measurementCalc'
    | 'measurementClose'
  args: string[]
  /** Raw file bytes; only `measurementOpen` reads it, and it is transferred. */
  bytes?: Uint8Array
}

export type EngineResponse =
  | { id: number; ok: true; result: string }
  | { id: number; ok: false; error: string }

// The tsconfig compiles against the DOM lib (the worker file shares the app's
// program), where `self` is a Window; narrow it to the two members a dedicated
// worker actually uses instead of dragging in the conflicting webworker lib.
const ctx = self as unknown as {
  onmessage: ((event: MessageEvent<EngineRequest>) => void) | null
  postMessage(message: EngineResponse): void
}

// Kick off wasm instantiation immediately so it overlaps the first request.
// `new URL(..., import.meta.url)` lets Vite emit the .wasm as a hashed asset
// and rewrite the URL in both dev and build.
const ready = init({
  module_or_path: new URL('./pkg/frees_wasm_bg.wasm', import.meta.url),
})

// `onmessage` is typed as returning void, so the async body is wrapped and
// its promise explicitly discarded: every failure is already turned into an
// error response inside `handle`, so there is nothing left to await on.
ctx.onmessage = (event: MessageEvent<EngineRequest>) => {
  void handle(event)
}

const handle = async (event: MessageEvent<EngineRequest>) => {
  const { id, method, args, bytes } = event.data
  try {
    await ready
    let result: string
    switch (method) {
      case 'solve':
        result = solve(args[0] ?? '', args[1] ?? '')
        break
      case 'check':
        result = check(args[0] ?? '', args[1] ?? '')
        break
      case 'reference':
        result = reference()
        break
      case 'version':
        result = version()
        break
      case 'fluids':
        result = fluids()
        break
      case 'propertyDiagram':
        result = property_diagram(args[0] ?? '', args[1] ?? '')
        break
      case 'psychrometricChart':
        result = psychrometric_chart(args[0] ?? '')
        break
      // The REPL evaluates against the workspace the last successful `solve`
      // left in this module, so both calls must reach the *same* worker
      // instance as the solve did. They do: engineClient keeps one.
      case 'replEvaluate':
        result = repl_evaluate(args[0] ?? '')
        break
      case 'replClear':
        repl_clear(args[0] ?? 'null')
        result = ''
        break
      // The opened file lives in this module for as long as the tab does, so
      // every measurement call must land on the same worker instance — the
      // same constraint the REPL has, satisfied the same way.
      case 'measurementOpen':
        // No silent empty parse: a missing `bytes` is a caller bug, and a
        // zero-byte open would surface as a confusing "not an MDF4 file".
        if (!bytes) throw new Error('measurementOpen requires the file bytes')
        result = measurement_open(bytes, args[0] ?? '')
        break
      case 'measurementChannelWindow':
        result = measurement_channel_window(args[0] ?? '')
        break
      case 'measurementCalc':
        result = measurement_calc(args[0] ?? '')
        break
      // Whatever the boundary answers is discarded, as `repl_clear`'s is:
      // deleteMeasurement is fire-and-forget, so there is no one left to tell.
      case 'measurementClose':
        measurement_close(args[0] ?? '')
        result = ''
        break
      default:
        throw new Error(`Unknown engine method: ${String(method)}`)
    }
    ctx.postMessage({ id, ok: true, result })
  } catch (e) {
    ctx.postMessage({
      id,
      ok: false,
      error: e instanceof Error ? e.message : String(e),
    })
  }
}
