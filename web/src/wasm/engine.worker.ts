// Module worker hosting the frees WASM engine off the UI thread.
//
// Protocol (see engineClient.ts, the only sender):
//   request  {id, method: 'solve' | 'solveTable' | 'check' | 'reference' |
//                     'version' | 'fluids' | 'propertyDiagram' |
//                     'psychrometricChart' | 'replEvaluate' | 'replClear' |
//                     'monteCarlo' | 'measurementCalc',
//             args: string[]}
//   response {id, ok: true, result: string} | {id, ok: false, error: string}
//
// `result` is the raw JSON string the wasm boundary emits (a REST-shaped
// SolveResponse/CheckResponse/LanguageReference; a bare semver string for
// 'version') — parsing happens on the client side, so the worker only ever
// posts strings.
//
// MDF4 reading was removed (decision D6): the engine no longer holds opened
// recordings, so the protocol is strings-only again. `measurementCalc`
// remains — the Data Analyzer's calculated signals evaluate frees formulas
// over inline series sampled from CSV imports held in the frontend's
// channelStore.
//
// Failure discipline: nothing may kill the worker. The wasm boundary already
// returns every *document* problem as data; this dispatch wraps the rest
// (init failure, unknown method, an unexpected trap) in {ok: false}.

import init, {
  check,
  fluids,
  measurement_calc,
  property_diagram,
  psychrometric_chart,
  reference,
  repl_clear,
  repl_evaluate,
  monte_carlo,
  solve,
  solve_table,
  version,
} from './pkg/frees_wasm.js'

export interface EngineRequest {
  id: number
  method:
    | 'solve'
    | 'solveTable'
    | 'monteCarlo'
    | 'check'
    | 'reference'
    | 'version'
    | 'fluids'
    | 'propertyDiagram'
    | 'psychrometricChart'
    | 'replEvaluate'
    | 'replClear'
    | 'measurementCalc'
  args: string[]
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
  const { id, method, args } = event.data
  try {
    await ready
    let result: string
    switch (method) {
      case 'solve':
        result = solve(args[0] ?? '', args[1] ?? '')
        break
      case 'solveTable':
        result = solve_table(args[0] ?? '', args[1] ?? '')
        break
      case 'monteCarlo':
        result = monte_carlo(args[0] ?? '', args[1] ?? '')
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
      case 'measurementCalc':
        result = measurement_calc(args[0] ?? '')
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
