# Status — Phases 0–2 complete, Phase 3 boundary wired

**Date:** 2026-07-30 · Workspace: 644 tests green, clippy `-D warnings` clean,
`cargo fmt` clean, wasm release bundle **397 KiB raw / 175 KiB gzipped**
(budget 2 MiB).

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
The ones that remain, ranked:

1. **No symbolic-Jacobian path** (`newton.rs`, critical, open). Java
   differentiates residuals via `Differentiator` first and falls back to finite
   differences; this port is FD-only. Documents that are FD-fragile may diverge.
   Unblocks when `Differentiator` ports in Phase 4.
2. **No solve retry ladder** (`engine.rs`, open). Java retries failed blocks
   with relaxed settings/merging/polish. One attempt per block here.
3. **Bounds are advisory** (`newton.rs`, open). Java clamps candidates into
   `[lo, hi]` inside the line search and Jacobian perturbation; here bounds only
   seed the start point, and out-of-bounds solutions warn.
4. **`#` constants stay `Expr::Var`** until `engine.rs` resolves them as knowns
   (Java folds them at parse time via `ConstantsRegistry`). Same results;
   different AST shape. A ConstantsRegistry module would align it.
5. **`Solution` has no `display_names`** (first-seen spellings). Golden fixtures
   record them; parity currently folds case instead of comparing them.
6. **Newline tolerance inside `[...]`/`(...)`** in two spots (`multiAssign`
   outputs, CALL args) where ANTLR would reject — Rust is more permissive.
7. **NaN in five-argument `If`** errors here, silently takes a branch in Java.
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
