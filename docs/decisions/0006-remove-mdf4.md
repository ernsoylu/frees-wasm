# D6 — Remove MDF4; the Data Analyzer is CSV-only

**Status:** Decided · **Date:** 2026-08-05

## Decision

Remove `.mf4` (ASAM MDF4) reading from the product: the `mdf4.rs` reader and
the `mf4-rs` dependency from `frees-core`, the `measurement_open` /
`measurement_channel_window` / `measurement_close` exports and the opened-file
registry from the wasm boundary, and the remote-source path from the web
analyzer. **The Data Analyzer itself stays**: CSV/TSV import (main-thread,
in-browser), the oscilloscope/instruments, multi-file compare, and
**calculated signals** — `measurement_calc` remains on the boundary, now
stateless, with every input series riding inline from the frontend's
channelStore.

## Context

The user's direction is to clip features; MDF4 was named. On the merits it
was the right candidate:

* **Phase 10's own status doc ranked its cost first**: compressed (`##DZ`)
  recordings — deflate, ZSTD, LZ4, *"the norm for OEM recordings"* — were
  refused, as were VLSD strings and multi-group files. The honest summary was
  "an OEM recording will probably not open". What shipped was the narrow
  uncompressed slice, and the sidecar that used to catch the rest has no
  browser equivalent.
* **It carried the repo's only supply-chain debt**: `mf4-rs → meval →
  nom 1.2.4`, the workspace's single future-incompat warning. Both recorded
  exit strategies (fork mf4-rs / vendor a narrowed reader) were work; removal
  is neither, and the warning is gone from every build.
* **It was the most-attacked surface in the tree**: of Phase 10's fifteen
  defects, the five allocation aborts and both unbounded walks lived in the
  MDF4 block-graph pre-flight. That hardening was real, but the surface it
  defended is now absent rather than defended.
* **CSV covers the workflow**: every measurement tool exports CSV, the CSV
  path never left the browser anyway, and the analyzer semantics
  (envelope decimation, gaps-are-NaN, calculated signals through the real
  frees expression language) are unchanged.

## What physically left

* `crates/frees-core/src/measurement/mdf4.rs` (2 024 lines) and `mf4-rs`
  from both manifests — `meval` and `nom 1.2.4` fall out of `Cargo.lock`.
* The boundary's registry (LRU, `RETAINED_BYTES_BUDGET`, channel cache) and
  three of its four measurement exports; a calc input arriving with a
  measurement *reference* now gets a typed error saying recordings are no
  longer held in the engine.
* The worker protocol's `bytes` channel and the transfer-list machinery in
  `engineClient` — the protocol is strings-only again.
* The analyzer's remote-source variant (window fetch cache, 404-eviction,
  `.mf4` accepts and import branch).
* `fixtures/measurement/a_small_uncompressed.mf4` (the repo's only binary
  fixture) and the MDF4 robustness/fuzz/parity tests, including three of
  Phase 10's fifteen defect regressions and both Phase 12 MDF4 fuzz
  properties — removed *with their surface*, not weakened.

## Consequences

* The wasm bundle sheds the `mf4-rs`/`mdf4.rs` share (Phase 10 measured it
  at 88.4 KiB raw) plus the registry code — headroom against the budget grows
  for the first time since Phase 6.
* A saved project that references an `.mf4` file still loads (the file-refs
  shape is unchanged); its channels show the existing "Locate file…"
  placeholder, and the picker now offers CSV only. No project-format
  migration.
* The Phase 10 status doc remains as history; its headline ("a `.mf4` opened
  in frees never leaves the machine") is superseded by a stronger sentence:
  measured data enters as CSV and *still* never leaves the machine.
* **Revisit if** OEM-format support returns as a requirement — the honest
  path then is the one Phase 10's gap list already priced: a reader with
  real decompressors (deflate + ZSTD + LZ4), likely as a lazily-loaded
  chunk, not a return of the narrow uncompressed slice.
