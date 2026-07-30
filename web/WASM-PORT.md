# WASM port notes for this vendored frontend

This tree is vendored from `../frEES/frontend` (rsync snapshot, 2026-07-30,
excluding `node_modules/`, `dist/`, `.vite/`). Solve and Check now run
**in-browser** through the WASM engine; the remaining `/api/*` endpoints are
stubbed in `src/api.ts` until their engine features port.

## The generated wasm package (`src/wasm/pkg/`)

`src/wasm/pkg/` is **generated output** (gitignored) and must exist before
`npm run dev`/`npm run build`. Rebuild it whenever `crates/` changes:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
wasm-pack build ../crates/frees-wasm --release --target web --out-dir ../web/src/wasm/pkg
```

(paths relative to `web/`; absolute paths work too).

## Files the WASM port has touched

- `src/api.ts` — **done**: `solve`/`check` call the engine worker via
  `src/wasm/engineClient.ts` and keep their exported signatures; every other
  endpoint is a stub (neutral boot values or a "not yet available in the
  browser engine" rejection). `runCompute`/`pollJob` are kept, unwired, for a
  future hybrid remote path.
- `src/wasm/` — **done**: `engine.worker.ts` (module worker hosting the wasm
  engine, `{id, method, args}` → `{id, ok, result|error}`) and
  `engineClient.ts` (lazy singleton, id correlation, typed
  `wasmSolve`/`wasmCheck`/`wasmVersion`).
- `src/defaultExample.ts` — **done**: boots a document the browser engine
  solves; the EV COMPONENT model is kept as `EV_THERMAL_EXAMPLE_TEXT` for
  Phase 6.
- `src/analyzer/measurementApi.ts` — untouched (Data Analyzer is Phase 10; its
  fetch failures are already user-visible upload errors).
- `vite.config.ts` — untouched so far: Vite's stock `new Worker(new URL(...))`
  handling bundles the worker and the `.wasm` asset without config.

## Everything else stays in sync with upstream

Do not fork other files. When upstream `../frEES/frontend` changes, re-run the
vendor rsync (same excludes) and re-apply only the files listed above; any
other local edit here is a bug.

## Node version

`web/.nvmrc` pins Node 22 (`nvm use` in this directory). Node 20.x builds the
app fine but **cannot run the vitest suite**: jsdom@30 pulls undici@8, whose
`CacheStorage` constructor calls `webidl.util.markAsUncloneable` — added in
Node 22 — so every test file fails at environment setup with
`TypeError: webidl.util.markAsUncloneable is not a function`. CI reads the
`.nvmrc` via `setup-node`'s `node-version-file` and runs `npx vitest run`
before the build. Verified locally: 33 files / 324 tests pass on v22.23.2.
