# D10 — Remove the spreadsheet; the Tables workbook becomes a native grid

**Date** 2026-08-23 · **Status** accepted, in implementation (Wave H)

## Context

Univer (`@univerjs/presets` + `@univerjs/preset-sheets-core`) is the app's
largest dependency by every measure: **5.18 MiB raw / 1.41 MiB gzipped —
35.9 % of the offline precache** (dist 14.43 MiB → 9.26 MiB without it),
175 MB of `node_modules` across 73 packages, plus two custom Vite build
plugins, a precache ignore list, and an 8 MiB `maximumFileSizeToCacheInBytes`
override that exist only to tame it. It serves two features:

* **`SpreadsheetTab`** — free-form spreadsheets with `ssheet()` document
  references, input bindings (`VAR = ssheet(...)` auto-equations with unit
  re-attach), result bindings with post-solve write-back, and
  create-table-from-selection.
* **`TablesWorkbookTab`** — the Tables workbook hosting function tables and
  GUI parametric tables as bound sheets (code-parametric and ODE tables
  already render in `glide-data-grid` windows).

Rebuilding only Tables would save ~20 kB — the win requires removing both.
The owner judged the free-form spreadsheet not needed; the Tables feature is
core and must lose nothing.

Two facts make the cut clean. The table model was already Univer-isolated by
upstream's own design (`tables.ts` + `tableBinding.ts` speak a stored-cell
format, "univerAdapter's isolation principle"; specs are the persisted truth,
sheets are materialized views). And every *table function* is engine-side —
`TABLE`/`PARAMETRIC`/`STATE TABLE` blocks, the nine parametric accessors, the
classic `Interpolate1`/`Lookup`/`DTable` set with log-space and natural-cubic
interpolation (`curvetable.rs`) — all parity-locked and untouched by any UI
swap.

## Decision

1. **Remove the spreadsheet feature entirely**: `SpreadsheetTab`, the
   `ssheet()` substitution pipeline, input/result bindings, and both Univer
   packages (plus the Univer-only `rxjs` dep and `lodash-es` override, the
   two Vite plugins and the precache overrides).
2. **Rebuild the Tables workbook on `@glideapps/glide-data-grid`** — already
   shipped (312 kB) for the read-only table windows, so the replacement adds
   zero vendor bytes. The ~60-item capability checklist from the removal
   survey is the acceptance list; the generic Univer ribbon (formatting,
   number formats, fx bar) is deliberately not reproduced — the purpose-built
   toolbars are.
3. **Complete the `functionTables` port while we are here**: the boundary
   accepts and ignores `functionTables` today, so GUI function tables
   (Tables workbook, Graph Digitizer) are not callable in equations in the
   browser — the Java injects them into the definition map
   (`SolveController.functionDefsOf`, lines 217/412/531, per-row in table
   solves included). Wiring this is parity-*completing*, not a divergence,
   and is what makes the cross-functional features (sweep→function,
   digitizer→fit→function, CSV→function) possible as pure UI composition.

## What is lost, explicitly

Free-form spreadsheets; `ssheet()` (documents using it now fail loudly at
parse instead of silently substituting); input/result bindings;
create-table-from-selection. Cell formulas in table input cells are replaced
by the Fill Column dialog (frees's own `first:step:last` idiom) — stored
`spec.formulas` from old files are surfaced read-only, never silently
dropped.

## Compatibility policy

* `SpreadsheetSpec.sheets` celldata in existing `.frees` project files:
  loaded inert and preserved on save (the `linkedTableId` precedent) with a
  one-time notice; never destroyed silently.
* The persisted dock-layout id `table:univer-workbook` is **kept** by the
  replacement window so saved layouts keep their Tables window.
* `spec.formulas` stays in the schema; the grid shows the stored text
  read-only with a conversion hint.

## Traps (from the survey; violate none)

* `vite.config.ts` `readdirSync`s `@univerjs/engine-render` at config-eval
  time: the dependency removal and the plugin removal must land in the same
  commit or every vite command breaks.
* `flushTablesWorkbook()` must stay synchronous — a just-typed cell must
  reach the solve that follows it.
* `MobileLayout.tsx:113` hardcodes the workbook window id raw.
* The 37-case `tableBinding.test.ts` suite is the behavioural contract —
  retargeted to the new grid, not deleted.

## Consequences

Vendored-tree fork: all `src/spreadsheet/*` files were unmodified upstream;
removal makes them a permanent divergence. `WASM-PORT.md`'s re-sync
procedure gains a D5-clip-style paragraph (future rsyncs re-delete
`src/spreadsheet/` and re-apply the Tables routing). Measurements
(dist/precache/node_modules before → after) are recorded here at the purge
step. Engine-side, the `functionTables` injection is graded by an
equivalence oracle — a table injected via the request must answer bit-
identically to the same table written as a `TABLE` block — plus wasm-level
tests in the `solve_table.rs` style. One rule the implementation verified
against the Java and mirrors exactly, because it is easy to assume the other
way round: on a name collision the **document** definition wins on the
solve/check path (`EquationSystemSolver.withExtraDefs` — "source definitions
win on name collision"), and the **request** table wins only in the REPL's
cached defs (`computeSolve`'s `replDefs.putAll(functionDefs)`). The UI must
not present a GUI table as overriding a same-named `TABLE` block.
