# Status — Phases 0–2 complete, Phase 3 boundary wired

> **Historical.** This document records the state at the end of Phase 1–3 and
> is kept for its divergence ledger (below), which is maintained. For the
> current state read [`docs/status-phase4.md`](status-phase4.md) first.

**Date:** 2026-07-30 · Workspace at the time: 644 tests green, clippy
`-D warnings` clean, `cargo fmt` clean, wasm release bundle **397 KiB raw /
175 KiB gzipped** (budget 2 MiB). Current numbers are in `status-phase4.md`.

## What exists and works

| Piece | State |
|---|---|
| Lexer (`lexer.rs`) | Complete against `Frees.g4` lines 499–632. 74 tests. |
| Unit registry (`units/`) | Full table + expression parser, C/F offsets, SI display names. 41+ tests. |
| Expression parser (`parser/expr.rs`) | Full precedence chain, the `-10 [C]` sign-folding rule, matrix literals, named args, member access, depth-bounded. 92+ tests. |
| Top-level parser (`parser/toplevel.rs`) | Equations, FOR, CALL, SYMBOLIC, multiAssign, rangeAssign, GUESS; 11 block forms refused explicitly. 61+ tests. |
| Evaluator (`eval.rs`) | Data-driven registry; elementary intrinsics, constants, `sum`/`product` binding, lazy `if`, radians. Java-parity semantics for mod/round/signum/NaN. |
| Blocker (`solver/blocker.rs`) | Hopcroft–Karp matching + petgraph Tarjan, topological order property-tested over 300 random systems; Java causality diagnosis prose. |
| Newton (`solver/newton.rs`) | FD Jacobian, partial pivoting, step-halving; stop criteria matched to Java (250 iter, 1e-12, 25 trials). |
| Engine (`engine.rs`) | `solve`/`check` wiring parse → constants → GUESS → block → per-block Newton. |
| CLI (`frees-cli`) | `solve`/`check`, file or stdin, JSON out. |
| wasm boundary (`frees-wasm`) | `version`/`solve`/`check` as JSON-string exports; panic hook. |
| Parity harness | 17 golden fixtures from the **real Java engine** (`tools/golden-dumper`); `tests/parity.rs` replays them — **17/17 match**, and the test provably detects an injected divergence. |
| CI | fmt + clippy + tests + wasm build + bundle-size gate + parity replay. |

The canonical case solves **identically to the oracle**: `x = 4.694012391660914`,
`y = 3.802174371161316`, 1 simultaneous block, 6 iterations.

## Known divergences from Java (deliberate or open)

From the adversarial verification pass (25 findings; 11 fixed during the pass).
Items 1–8 as originally recorded; **five are now closed** — struck through with
the date and the reason, never silently deleted. Phase 4 closed items 1, 2 and
3; see [`docs/status-phase4.md`](status-phase4.md) for the full Phase-4 ledger,
including divergences that phase *opened* and did not close.

1. ~~**No symbolic-Jacobian path**~~ **Closed 2026-07-30 (Phase 4)**:
   `differentiator.rs` ports `Differentiator`, `engine.rs` pre-differentiates
   each block's dependent (equation, variable) pairs, and `newton.rs:294` uses
   the analytic Jacobian with the Java all-or-nothing fallback to finite
   differences. `asin`/`acos`/`atan` still return `None` and fall back to FD —
   that matches the Java switch and is intended.
2. ~~**No solve retry ladder**~~ **Closed 2026-07-30 (Phase 4)**: all three Java
   rungs are ported — `retryWithTransformedGuesses`,
   `tryUnivariateBracketingSolve`, `tryMergeBidirectional` — capped at
   `MAX_RETRY_ITERATIONS = 500`. Failures already carried the Java partial
   diagnostics. *Caveat:* the ladder can mask a kernel's own error message; see
   Phase-4 non-delivery 6 (non-symmetric Cholesky).
3. ~~**Bounds are advisory**~~ **Closed 2026-07-30 (Phase 4)**: candidates are
   clamped into `[lo, hi]` at all three Java `Math.clamp` sites — the Jacobian
   probe, `backtrackLineSearch`, and `dampedRescue`. Verified against the oracle
   with the root exactly *on* a bound, a root outside the bounds, and guess
   clamping.
4. ~~**`#` constants stay `Expr::Var`**~~ **Closed 2026-07-30**: the expression
   parser now folds built-in constants at parse time exactly like
   `AstBuilder.visitVarAtom` (value + raw SI unit string), so unit inference
   grounds through them (`v = g#*t` → m/s).
5. ~~**`Solution` has no `display_names`**~~ **Closed in Phase 3**: present on
   `Solution` and `CheckReport`; the parity test compares them exactly.
   (Phase 4 then found the map was *reconstructed* by a lexer pass rather than
   recorded where Java records it, and fixed that — 30 fixtures.)
6. **Newline tolerance inside `[...]`/`(...)`** in two spots (`multiAssign`
   outputs, CALL args) where ANTLR would reject — Rust is more permissive.
   **Still open**; not re-verified during Phase 4.
7. **NaN in five-argument `If`** errors here, silently takes a branch in Java.
   **Still open, deliberate** — pinned by
   `eval.rs::five_argument_if_refuses_a_nan_comparison_where_java_falls_through`.
8. **`x = x`** reports solvable and returns the default guess — Java parity
   (both engines do this); recorded for visibility.

Full detail: workflow output `wk1ueuu8a` findings list.

## Commands

```bash
export PATH="$HOME/.cargo/bin:$PATH"          # rustup-installed toolchain
cargo test --workspace                        # 644 tests incl. parity replay
cargo test -p frees-core --test parity        # golden-corpus parity only
cargo clippy --workspace --all-targets -- -D warnings
wasm-pack build crates/frees-wasm --release --target web --out-dir ../../pkg
tools/golden-dumper/run.sh                    # regenerate fixtures from Java
printf 'x = 2\ny = x^2\n' | cargo run -qp frees-cli -- solve
```

## Next (in dependency order)

1. **Phase 3 remainder — the browser vertical slice**: Web Worker protocol,
   `api.ts` fetch→RPC shim, vendor `web/` from `../frEES/frontend`, prove
   Editor → Check → Solve → Solution offline. This is the thesis milestone.
2. **Corpus growth**: harvest `examples.ts` documents into `fixtures/corpus/`
   and fix what diverges (the retry-ladder gap will surface here first).
3. **Phase 4**: `Differentiator` (unblocks the symbolic Jacobian), arrays &
   matrix intrinsics, `ComplexExpansion`, procedural bodies.
