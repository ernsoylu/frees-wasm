# D5 — Clip the dead-end UI instead of shipping stub promises

**Status:** Decided · **Date:** 2026-08-05

## Decision

Remove the UI affordances whose engine features exist only as
`NOT_IN_BROWSER_ENGINE` stubs in `api.ts`:

* the **Min/Max (optimization)** modal — single- and multi-objective,
* the **Curve Fit** modal,
* the **PID Tuner** modal, including the schematic/Workspace "Tune…" button
  on `SigPID` rows and the plant auto-extraction flow,
* the **Monte Carlo Uncertainty** modal,
* the **Parameter Estimation** modal,
* the **PDF/EPS plot export** options (the server-side Apache FOP transcode;
  SVG remains and is fully vector, client-side).

Keep, deliberately:

* **the `api.ts` stubs themselves and their tests** — `WASM-PORT.md`'s
  contract is that `api.ts` keeps its exported surface as the wiring seam for
  the engine features (Phase 8's `analysis/` module is written and
  unit-tested; only the boundary is missing) and for the optional remote
  adapter;
* **`pidLoop.ts` / `pidGainRewrite.ts` and their tests** — ported, working
  logic the tuner will need on the day `pidTune` is wired;
* **the Tables workbook** — its GUI Solve path is also a stub
  (`solveTable`), but the workbook is core UI whose Check path and
  document-level `TABLE`/`PARAMETRIC` blocks work; it is a wire-next
  candidate, not a clip. *(Wired 2026-08-22, Wave B1 — the first affordance
  to take the reverse migration this decision kept open: the wasm
  `solve_table` export drives `analysis::parametric::run_sweep` behind the
  transcribed controller caps, `api.ts`'s stub became the real call, and the
  workbook Solve works in-browser. Eight `NOT_IN_BROWSER_ENGINE` stubs
  remain.)* *(Wave B2, 2026-08-22: `runMonteCarlo` follows — the wasm
  `monte_carlo` export drives `analysis::montecarlo::run` (seeded, JavaRandom,
  the 2–1000 sample cap and 120 s truncating budget transcribed), and
  `MonteCarloModal.tsx` is restored from this decision's own commit
  (`git show 7f049be^:…`) with its launch point in the Tools menu — the
  "one-line change per launch point" claim held. Seven stubs remain.)*
  *(Wave B3, 2026-08-22: the remaining analysis quartet follows — `optimize`,
  `optimize_multi`, `curve_fit` and `parameter_fit` exports with the
  OptimizeController's validation transcribed verbatim (the shared
  "No equations entered."/"Syntax error: " idioms, the [40, 200]
  population/generations clamps, curve-fit's ordered field checks, the
  parameter-fit [10, 300] evaluation clamp, 200 000-sample ceiling and 120 s
  budget), and MinMaxModal, CurveFitModal and ParameterFitModal restored from
  this decision's commit into a Tools → Analysis section. Three stubs remain:
  `exportVector` (no browser FOP), `pidTune` and `extractPlant` — Wave B4.)*

## Context

The user's direction for this build is to clip features rather than carry
promises. Every removed affordance opened a fully-rendered modal whose
primary action could only ever surface "not yet available in the browser
engine" — the worst kind of UI: it looks like a feature, costs a lazy-loaded
chunk, and delivers an apology. Phase 10's status doc had already called this
out (gap 11); Phase 12 confirmed nothing behind it moved.

Clipping is cheap to reverse: the modals live in git history, the stubs and
helpers remain, and re-adding a menu item is a one-line change per launch
point. Wiring, by contrast, is the Phase 8 backlog (optimizer, NSGA-II,
curve fitter, Monte Carlo, parameter fit) plus `pidTune`/`extractPlant`
(control-side, mostly built) — real engineering with real acceptance tests
already staged in `corpus-pending/`.

## Consequences

* Five modal components deleted (`MinMaxModal`, `CurveFitModal`,
  `PidTunerModal`, `MonteCarloModal`, `ParameterFitModal`); their launch
  points removed from the Tools menu, the left rail, Spotlight and the
  Workspace component rows. The service-worker precache dropped 334 → 328
  entries.
* The UI now promises nothing the engine cannot do. The reverse migration
  (wiring) starts from the stubs, not from this decision.
* The Help pages still *describe* some clipped features; they document the
  frees language and its upstream capabilities, and editing vendored docs is
  out of scope for this pass. Recorded as the known rough edge.
* Ledger item 36 records the product-surface divergence from upstream.
