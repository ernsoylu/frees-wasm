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

> Those counts are the snapshot at the time this document was written. Item 4
> ("Verification never ran") and parts of items 3 and 7 have since been done —
> see **[Adversarial verification pass](#adversarial-verification-pass-item-4--what-it-found)**
> below for the current numbers and the three divergences it found.

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
   * ~~`expand_complex` cannot propagate display names; A5 added
     `expand_with_display_names` as a workaround — engine should call it.~~
     **Done** — `solve_with` now calls `expand_with_display_names` in complex
     mode and folds the result back into the document map, so `Zed = 3 + 4i`
     reports `zed_r → "Zed_r"` rather than `zed_r → "zed_r"`.
   * A5 notes `_i` components must seed at 0.0 and `_r` at the base guess
     (Java `complexComponentSpec`) — engine seeding not yet done.
   * `latex.rs` carries a local `ResidueResult`; relocate to `cas` when it ports.
   * A1: `asin`/`acos`/`atan` deliberately return `None` from `differentiate`
     (matching the Java switch), so those fall back to FD.

## Adversarial verification pass (item 4) — what it found

Item 4 above ("Verification never ran") has now run for the Java-parity half.
Method: author a document, run it through **both** engines
(`tools/golden-dumper/run.sh` for Java, `cargo run -qp frees-cli -- solve` for
Rust), diff values, variable names, block counts and error classifications.
~70 probe documents across the whole Phase-4 surface. Three genuine divergences
turned up; all three were "the kernel exists, the wiring does not", and all
three are fixed.

**1. `display_names` was reconstructed instead of recorded.** The port rebuilt
`ParseResult.displayNames` from a lexer pass over the source and then filtered
it to the solved variables. Both halves were wrong. Unit spellings and TABLE
column headers are `Ident` tokens, so they won the first-seen race against real
variables — `cp = 1004 [J/kg-K]` bound `k -> "K"`, beating a later `k = 1.4`,
and `TABLE htc(re)` beat a later `Re = 2000`. In the other direction, FUNCTION /
PROCEDURE / MODULE body-locals and formals, a `GaussIntegral`'s integration
variable, bare array/matrix container names, and the original casing of expanded
elements (`A[1,1]`) were all absent. The map is now accumulated exactly where
Java accumulates it: `Cursor::record_display_name` at the two `AstBuilder`
registration sites (`visitVarAtom`, `visitArrayAtom` — *not* `visitMemberAtom`,
*not* call names), `procedures::flatten_calls` for MODULE-namespaced variables,
and the element rule (`getOrDefault(base, base) + "[i,j]"`) replayed over the
expanded system. This alone promoted **30** staged fixtures.

**2. `det$<n>` was emitted but never dispatched.** `parser::expand` emits
`det$<n>` for any `det(A)` larger than 3×3 (the closed-form cofactor expansion
is O(n!)), and `linalg::eval_intrinsic` implements it along with `qr$`, `chol$`,
`expm$` and `svd$` — but `eval::eval_synthetic` had no arm for any of them, so
the entire module was unreachable from user text and `det` of a 4×4 died with
"not yet supported: det$4". Verified against the oracle for a diagonal 4×4, a
pivot-swap sign flip, and a singular 5×5 (LU convention: exactly `0.0`).

**3. `CALL LUDecompose` / `CALL Interp2` were refused before their flatteners
ran.** Java flattens PROCEDURE/MODULE calls and the matrix intrinsics in one
pass (`flattenCallProc`); this port splits that across `procedures::flatten_calls`
(stage 2) and `parser::expand` (stage 3), and stage 2 refused *every* intrinsic
CALL by name — including the two whose flatteners live in stage 3. Both
`expand::flatten_lu_decompose` and the new `expand::flatten_interp2` were
dead code. `procedures::EXPANDED_CALL_TARGETS` now passes them through.

### Verified as already correct (no divergence found)

Probed and bit-identical to the oracle: matrix element naming and display case,
row `[1, 2]` vs column `[1; 2]`, `inv`/`det` aliases, transpose, elementwise
operators, cross product, explicit-index matrix products, scalar broadcast;
FUNCTION/PROCEDURE/MODULE scoping (body locals do not leak, formals shadow,
dynamic scoping into the caller, `<name>$<n>$` namespacing and its instance
numbering); TABLE XLOG/YLOG interpolation in log space, clamping at both ends,
curve-family blending and parameter clamping, descending and duplicate x
columns, single-point tables, a table name shadowing a variable; `Integral`
step error (`1.9999993342983777` for `sin` over `[0, π]` — identical), the
integration-variable pin value, reversed limits, degenerate limits, variable
limits, `FUNCTION`/`TABLE` integrands, nested integrals, `GaussIntegral` point
clamping; solver bounds with the root exactly *on* a bound, a root outside the
bounds, guess clamping, root selection on multi-root polynomials (`x^7 = 128`,
`x^2 = 9` from a negative guess, a cubic with three real roots), near-singular
Jacobians, topological block ordering; unit offsets, unknown units, dimension
mismatches; operator precedence and `^` right-associativity.

### Still open, recorded not fixed

* **`module_inside_for_loop`** — Java unrolls `FOR` *during* flattening, so a
  MODULE CALL in a two-iteration loop yields `twice$1$…` and `twice$2$…`. This
  port flattens CALLs before unrolling and refuses the shape loudly instead.
  Fixing it means moving MODULE flattening past the unroller and re-basing the
  shared instance counter — not a local change. The golden is staged.
* **`CALL FFT/IFFT/Convolve/LinFit/PolyFit/QR/Cholesky/SVD/MatExp`** — the
  kernels (`signal`, `statistics`, `linalg`) and the `$`-synthetic dispatch all
  exist and are unit-tested, but `parser::expand` has no flattener for these
  eight, so they are still refused at stage 3. Each is a small, self-contained
  port of the corresponding `EquationParser.flattenX`.
* **`solver_singular_linear_cycle`** — held deliberately: the system is
  structurally square but rank-deficient, so its solution set is a line and any
  golden freezes an arbitrary point of a continuum.

```
1305 Rust tests (was 1174)   clippy -D warnings clean   fmt clean
161/161 golden fixtures match the Java oracle (was 17)
36 documents still staged in fixtures/corpus-pending, of which 9 are
  deliberately withheld (8 CoolProp-poisoned goldens + 1 rank-deficient system)
web 328 tests + vite build green
```

`cargo test --release --workspace` is the practical form of the gate now — the
parity replay solves 161 documents and takes ~80 s under a debug build.

## Resuming

The workflow script is saved and resumable — cached agents replay instantly:

```
Workflow({ scriptPath: ".../frees-wasm-phase4-wf_4c69e7a9-7ed.js",
           resumeFromRunId: "wf_4c69e7a9-7ed" })
```

Resuming re-runs only the 9 failed/never-run agents. Prefer starting with the
integrate stage's job list (item 1 above) — the implementation agents' output is
already on disk and committed.
