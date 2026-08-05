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

(paths relative to `web/`; absolute paths work too). The Dockerfile guards
this: an image build fails fast when the pkg is absent instead of shipping a
UI with no engine.

## Files the WASM port has touched

- `src/api.ts` — **done**: `solve`/`check` call the engine worker via
  `src/wasm/engineClient.ts` and keep their exported signatures; every other
  endpoint is a stub (neutral boot values or a "not yet available in the
  browser engine" rejection). `runCompute`/`pollJob` are kept, unwired, for a
  future hybrid remote path (opt-in via `VITE_API_BASE`).
- `src/wasm/` — **done**: `engine.worker.ts` (module worker hosting the wasm
  engine, `{id, method, args}` → `{id, ok, result|error}`) and
  `engineClient.ts` (lazy singleton, id correlation, typed
  `wasmSolve`/`wasmCheck`/`wasmVersion`).
- `src/defaultExample.ts` — **done**: boots a document the browser engine
  solves; the EV COMPONENT model is kept as `EV_THERMAL_EXAMPLE_TEXT` for
  Phase 6.
- `src/analyzer/measurementApi.ts` — **done (Phase 10)**: all four
  measurement routes call the engine worker; a `.mf4` never leaves the tab.
- `vite.config.ts` — forked: `buildInfoPlugin`, the vendor `manualChunks`
  split, `rollup-plugin-visualizer`, and (Phase 11) `vite-plugin-pwa`. The
  `/api` dev proxy is gone — nothing in `src/` calls a live endpoint.

## Files Phase 11 added or forked (browser-native product layer)

- `src/projectStore.ts` + `src/ProjectLibraryModal.tsx` — the IndexedDB
  project library and durable autosave mirror (decision D4);
  `src/project.ts` gains `normalizeStoredProject` so both storage backends
  share one sanitize/migrate trust boundary. Wired into `src/App.tsx`,
  `src/WorkspaceChrome.tsx`, `src/MobileLayout.tsx`.
- `src/pwa.tsx` + `src/main.tsx` — service-worker registration with a
  prompt-style update flow (a background activation must not yank hashed
  chunks from under a live tab; see the comment in `vite.config.ts`).
- `index.html` — theme-color, SVG favicon, apple-touch-icon; the manifest
  link is injected by the PWA plugin.
- `public/icons/` — the app icon set (SVG sources + rasterized PNGs).
- `nginx.conf.template` + `Dockerfile` — static-only: the `/api` proxy
  blocks, rate limiting and real-ip machinery went with the server.

## The feature clip (decision D5, 2026-08-05)

The Min/Max, Curve Fit, PID Tuner, Monte Carlo and Parameter Estimation
modals and the PDF/EPS export options are **deliberately removed**, not
missing from the vendor sync: their engine features exist only as
`NOT_IN_BROWSER_ENGINE` stubs, and this build's UI promises only what its
engine does. Files deleted: the five `*Modal.tsx` components. Files forked
for the clip: `App.tsx`, `WorkspaceChrome.tsx`, `Workspace.tsx`,
`plots/exportPlot.ts`. The `api.ts` stubs and the `pidLoop`/`pidGainRewrite`
helpers stay as the wiring seam. When re-syncing from upstream, re-apply the
clip; when wiring a feature, restore its modal from git history and delete
this paragraph's entry for it.

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
before the build. Verified locally: 39 files / 388 tests pass on v26.5.0
(Phase 11; the Phase 10 run was 38/369 on v22.23.2).
