# Phase 4 — PARTIAL. What landed, what did not.

**Date:** 2026-07-30 · **State:** all gates green, phase incomplete.

The Phase-4 workflow (12 agents) hit the account session limit mid-run.
3 agents finished cleanly, 6 died with substantial work already written to
disk, and the **integrate + 2 verify stages never ran at all**. This document
is the honest inventory — read it before continuing Phase 4.

```
1174 Rust tests (was 749)   clippy -D warnings clean   fmt clean
17/17 golden fixtures match the Java oracle
wasm 955 KiB raw / 372 KiB gzipped (budget 2048 KiB raw)
web 324 tests + vite build green
```

## Agent outcomes

| Agent | Files | Outcome |
|---|---|---|
| A1 differentiator | `differentiator.rs`, `tests/differentiator.rs` | **Finished** — 92 tests |
| A5 complex | `parser/complex.rs`, `tests/complex.rs` | **Finished** — 33 tests |
| A7 latex | `parser/latex.rs` | **Finished** — 24 tests |
| A2 solver hardening | `solver/newton.rs`, `engine.rs`, `tests/solver_hardening.rs` | **Died** — substantial work landed |
| A3 matrix | `parser/expand.rs`, `linalg.rs`, `tests/matrix_expansion.rs` | **Died** — substantial work landed |
| A4 procedural | `parser/toplevel.rs`, `procedures.rs`, `tests/procedural.rs` | **Died** — substantial work landed |
| A6 eval + kernels | `eval.rs` (+6268 lines), `curvetable.rs`, `statistics.rs`, `signal.rs`, `interp2.rs` | **Died** — substantial work landed |
| A8 integral | `integral.rs` | **Died** — state unverified |
| A9 corpus | `fixtures/corpus-pending/**` | **Died** — nothing produced |
| Integrate | — | **Never ran** |
| Verify ×2 | — | **Never ran** |

## Stabilisation applied here (not by the agents)

1. **Cross-agent seam**: `tests/complex.rs` (A5) called the 3-arg
   `newton_solve`; A2's refactor had already made it 4-arg with bounds.
2. **A6 self-inconsistency**: `besselj`/`bessel_j` were implemented *and* still
   listed as unported, failing A6's own two consistency tests. The whole Bessel
   family is in fact implemented; the stale entries were removed.
3. **76 clippy errors** in unfinished agent code (they never reached a raw
   gate). `clippy --fix` handled the mechanical ones; the rest got **justified
   module allows** rather than "fixes" that would have been wrong:
   * `approx_constant` — `0.636619772` is Numerical Recipes' truncated 2/π,
     transcribed from `Evaluator.java`. Substituting `FRAC_2_PI` changes the
     value and **breaks bit-parity with the oracle**.
   * `neg_cmp_op_on_partial_ord` — `!(x > 0.0)` guards reject NaN; `x <= 0.0`
     does not. The NaN behaviour is the point.
   * `mut_range_bound` — Apache's `rjbesl` reassigns the scan bounds to drive
     an inner loop, then breaks. Correct as transcribed.
   * `needless_range_loop` — matrix kernels index parallel arrays by one
     variable, mirroring the Java/Fortran source.

## NOT done — the Phase-4 completion list

Ranked. Items 1–3 are the reason this phase cannot be called finished.

1. **Pipeline wiring never happened.** `engine.rs` does *not* yet run
   `flatten_calls → expand_complex → expand_document` in the solve path, and
   residuals are not evaluated through `eval_with`/`EvalContext{defs}`. So:
   matrix documents, user `FUNCTION`s and `TABLE` lookups **do not work
   end to end** even though every piece exists. This is the single highest-value
   next task, and it is mostly mechanical.
2. **No end-to-end feature tests.** Each module is unit-tested in isolation;
   nothing exercises a FUNCTION inside a matrix equation, a TABLE lookup inside
   a Newton block, or an Integral with an unknown bound.
3. **No corpus growth, no fixture promotion.** A9 produced nothing, so the
   parity gate is still the 17 Phase-1 fixtures. `web/src/examples.ts` remains
   unharvested — the retry-ladder and matrix gaps will only surface there.
4. **Verification never ran.** No Java-parity adversary (matrix element naming,
   MODULE namespacing, TABLE log interpolation, complex `_r`/`_i` display,
   integral accuracy, retry-ladder rung order, bounds-at-boundary) and no
   robustness fuzzing of the new surface (recursive FUNCTIONs, `WHILE true`,
   huge matrix literals, degenerate tables).
5. **`getReference` still stubbed** — Help/autocomplete show nothing.
6. **A8's `integral.rs` unverified.** It compiles and the suite is green, but
   no one confirmed the quadrature matches the Java scheme.
7. **Known contract debts** recorded by the finished agents:
   * `expand_complex` cannot propagate display names; A5 added
     `expand_with_display_names` as a workaround — engine should call it.
   * A5 notes `_i` components must seed at 0.0 and `_r` at the base guess
     (Java `complexComponentSpec`) — engine seeding not yet done.
   * `latex.rs` carries a local `ResidueResult`; relocate to `cas` when it ports.
   * A1: `asin`/`acos`/`atan` deliberately return `None` from `differentiate`
     (matching the Java switch), so those fall back to FD.

## Resuming

The workflow script is saved and resumable — cached agents replay instantly:

```
Workflow({ scriptPath: ".../frees-wasm-phase4-wf_4c69e7a9-7ed.js",
           resumeFromRunId: "wf_4c69e7a9-7ed" })
```

Resuming re-runs only the 9 failed/never-run agents. Prefer starting with the
integrate stage's job list (item 1 above) — the implementation agents' output is
already on disk and committed.
