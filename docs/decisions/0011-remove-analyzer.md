# D11 — Remove the Data Analyzer; measured data enters through Tables

**Date** 2026-08-24 · **Status** accepted, in implementation (Wave J)

## Context

The Data Analyzer is the last survivor of the measured-data line. Its history
is a steady retreat:

* **Phase 10** shipped it against `.mf4` recordings, with an engine-side
  measurement stack (`crates/frees-core/src/measurement/`, 3 251 lines:
  sampled series, envelope decimation, raster construction, calculated
  signals) and its wasm boundary (`crates/frees-wasm/src/measurement.rs`,
  1 184 lines, the `measurement_calc` export).
* **[D6](0006-remove-mdf4.md)** removed MDF4 outright, leaving the Analyzer
  **CSV-only** and `measurement_calc` alive but stateless.
* **Wave H** then gave measured data a second, better route: CSV → function
  table, callable in equations.

What remains is a six-instrument oscilloscope UI (~6 000 lines across 31
files, plus `uplot` and `papaparse`) whose plotting job the Plots feature
already does for everything the solver produces, and whose one irreplaceable
capability — getting measured numbers *into* a document — is now served more
directly by a function table.

## Decision

1. **Remove the Data Analyzer**: `web/src/analyzer/**` (all instruments,
   channel store, CSV worker, decimation, stats, compare, offsets, calc
   signals), its dock windows and Inspector panel, the `analyzers` state and
   its project-file persistence, and the `uplot` + `papaparse` dependencies.
2. **Relocate CSV import into the Tables workbook**: an "Import CSV…" action
   producing a **function table** through Wave H's existing
   `composeTables` path (x column, y column, name, the 5 000-row decimation
   guard). Measured data stays importable *and* becomes callable in
   equations — the capability the Analyzer's own CSV path only half
   provided.
3. **Remove the engine measurement stack too**: `measurement_calc`,
   `crates/frees-wasm/src/measurement.rs`, `crates/frees-core/src/
   measurement/`, and their `measurement_parity` / `measurement_robustness`
   suites. The Analyzer is their only consumer; with it gone they are
   unreachable code carrying wasm bytes. This completes what D6 began.

## What is lost, explicitly

The oscilloscope workflow: multi-channel strip charts with cursors, the six
instruments (compare, histogram, scatter, statistics, table, event list),
calculated signals (frees formulas evaluated over sampled series), per-signal
offsets/relocation, envelope decimation of large recordings, CSV export of
channel data, and importing a *solved table back into* the analyzer
(`tableImport.ts`, the oscilloscope-parity path). None has a Plots
equivalent; the judgement is that none is worth its cost now that measured
data reaches documents through Tables.

## Compatibility policy

Following D10's precedent exactly: an existing `.frees` project's
`analyzers` array is **parsed and re-serialized inert** — never destroyed —
and a one-time load notice says the feature was removed and the data is
preserved in the file. No migration attempts to convert analyzer sessions
into tables; the shapes are too different to guess at.

## Consequences

`web/` is a vendored tree, so this is a second permanent fork beside D10's:
`WASM-PORT.md`'s re-sync procedure gains the Analyzer to its re-delete list.
Engine-side this is the first removal of *ported parity code* — the
measurement module was a faithful Phase-10 port with its own oracle suites,
so the decision is recorded here rather than in a status doc, and the corpus
is untouched (no measurement fixture ever entered `fixtures/corpus`).
Before/after measurements (dist, precache, wasm module, `node_modules`) are
recorded at the purge step.
