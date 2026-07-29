# D3 — Threading model

**Status:** Decided · **Date:** 2026-07-29

## Decision

The engine is **single-threaded**. Parallelism comes from a **pool of
independent Web Workers**, each owning its own wasm instance. Do **not** use
`wasm-bindgen-rayon` / `SharedArrayBuffer` in v1.

## Context

Shared-memory threading in wasm requires `SharedArrayBuffer`, which browsers gate
behind cross-origin isolation:

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

Those headers are not free. They require control of the response headers — so
the app can no longer be dropped on arbitrary static hosting — and
`require-corp` breaks embedding of any third-party resource that does not opt in.
That is a large product constraint to accept for a v1.

What would we actually buy? The two candidate wins:

1. **A single large simultaneous block.** Newton on one block is inherently
   sequential — factorise, step, re-evaluate. The parallelism available is inside
   the Jacobian assembly (one residual evaluation per column) and the linear
   solve. Real, but bounded, and it only matters for large blocks.
2. **Parametric sweeps, Monte Carlo, NSGA-II.** These are *embarrassingly*
   parallel: independent runs with no shared state.

Case 2 is the bulk of the wall-clock a user actually waits on, and it needs no
shared memory at all. A worker pool covers it completely.

## Consequences

* The app deploys as **plain static files with no special headers** — a real
  product advantage the server-based parent does not have, and one the PWA path
  (Phase 11) depends on.
* Sweeps, Monte Carlo, and Pareto fronts (Phase 8) dispatch across a worker pool
  sized to `navigator.hardwareConcurrency`. Each worker holds an independent
  engine instance; results are collected by message passing.
* Per-worker memory is independent, which also spreads load against the wasm32
  4 GB ceiling instead of concentrating it.
* Engine code stays free of synchronisation primitives — `frees-core` has no
  `Send`/`Sync` requirements to satisfy and no locks to get wrong.
* **Revisit if** benchmarking (Phase 12) shows single-block solve time on
  realistic component networks is the dominant cost *and* intra-block
  parallelism would materially fix it. Even then, prefer a faster linear solve
  or the sparse path (`SparseSteadyKlu`'s successor) before taking on COOP/COEP.

## Note on the property-table backend

D1's precomputed `(P,h)` tables are read-only after generation. Under the
worker-pool model each worker gets its own copy, which costs memory but avoids
any sharing question. If table memory becomes the binding constraint, that is a
concrete, measurable reason to revisit — unlike a general appeal to "threads are
faster".
