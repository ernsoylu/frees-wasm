// Module worker hosting the frees WASM engine off the UI thread.
//
// Protocol (see engineClient.ts, the only sender):
//   request  {id, method: 'solve' | 'check' | 'version', args: string[]}
//   response {id, ok: true, result: string} | {id, ok: false, error: string}
//
// `result` is the raw JSON string the wasm boundary emits (a REST-shaped
// SolveResponse/CheckResponse; a bare semver string for 'version') — parsing
// happens on the client side, so the worker only ever posts strings.
//
// Failure discipline: nothing may kill the worker. The wasm boundary already
// returns every *document* problem as data; this dispatch wraps the rest
// (init failure, unknown method, an unexpected trap) in {ok: false}.

import init, { check, solve, version } from './pkg/frees_wasm.js'

export interface EngineRequest {
  id: number
  method: 'solve' | 'check' | 'version'
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

ctx.onmessage = async (event: MessageEvent<EngineRequest>) => {
  const { id, method, args } = event.data
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
      case 'version':
        result = version()
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
