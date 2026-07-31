# Status — Phases 0–2 complete, Phase 3 boundary wired

> **Historical.** This document records the state at the end of Phase 1–3 and
> is kept for its divergence ledger (below), which is maintained. For the
> current state read [`docs/status-phase6.md`](status-phase6.md) first, then
> [`docs/status-phase5.md`](status-phase5.md) and
> [`docs/status-phase4.md`](status-phase4.md).

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
including divergences that phase *opened* and did not close. Phase 5 closed
none of items 1–8 and **opened four of its own (9–12)**, recorded below with the
same rules; the full Phase-5 ledger is
[`docs/status-phase5.md`](status-phase5.md#what-phase-5-did-not-deliver).
Phase 6 closed none of items 1–12 and **opened two of its own (13–14)**; the
full Phase-6 ledger is
[`docs/status-phase6.md`](status-phase6.md#what-phase-6-did-not-deliver).

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

### Opened by Phase 5 (2026-07-30)

9. **Real-fluid properties come from precomputed tables, not CoolProp.** The
   engine answers `Enthalpy`/`Entropy`/`Density`/`Volume`/`Temperature`/`Q` and
   the four critical/triple constants for **water and R134a only**, from the
   `FRPHTAB1` artifacts `tools/table-gen` generates offline (decision D1). The
   measured error against CoolProp 8.0.0 is `1e-7…2e-4` relative; on the
   promoted fluid documents it is `6.4e-07…7.2e-05`. Transport properties,
   `Cpmass`/`Cvmass`, `Z`, speed of sound, Prandtl, surface tension, humid air,
   supercritical states, mixtures, incompressibles and all 34 other CoolProp
   fluids are **refused by name**, never approximated. **Open and structural** —
   closing it means shipping `coolprop.wasm` (D1 option A, still available).
10. **Five parity fixtures compare at a declared tolerance, not `1e-9`.** A
    direct consequence of 9: no table-backed engine can match full-accuracy
    CoolProp goldens at `1e-9`. `fixtures/tolerances.json` relaxes the *numeric*
    tolerance for `rankine-cycle`, `rankine-cycle-2`, `refrigeration-vcr`,
    `props_realfluid_water_states` and `props_realfluid_r134a_states`, each with
    its measured error and mechanism; `display_names`, `block_count` and error
    classification stay exact for all 268. Guarded: a stale or unnecessary entry
    fails the gate. **Open**; closed by the same move as 9.
11. **`plot_fluids()` is narrowed to what the backend can serve.** The Java
    returns all 36 canonical CoolProp names because CoolProp serves all 36;
    `GET /api/plot/fluids` here returns the intersection with
    `RealFluid::served_fluids()`, so the picker shows two. **Deliberate** — a
    list that fails on thirty-four of its entries is worse than a short one — and
    self-closing: a backend that serves everything returns `None` and gets the
    Java list back verbatim.
12. **The liquid-piece coordinate diverges from `SaturationSplitTable.java`.**
    The Java measures subcooling as `h_f(P) − h` capped at one depth valid at
    every served pressure — a cap set by the thinnest sliver, at low pressure —
    and falls through to a native call for anything outside it, which this port
    cannot. The shipped tables use a **normalized** depth
    `(h_f − h)/(h_f − h_cold)` instead, which follows the sliver at every
    pressure at identical byte cost and turns `rankine-cycle`'s 8 MPa pump-exit
    state from an uncovered miss into a `4.2e-06` hit. **Deliberate and
    additive**: both modes are implemented, the mode is a header flag, and
    `SaturationSplitTable::build` — the line-for-line port of the Java
    constructor — still produces the absolute one and nothing else.

### Opened by Phase 6 (2026-07-31)

13. **A hierarchical `COMPONENT` may not nest more than 64 subsystems deep.**
    `ComponentExpander.flattenInstance` recurses once per level and the Java has
    no guard: it dies with `StackOverflowError`, which a JVM turns into a
    catchable `Error` on a thread it can abandon. A wasm module cannot — measured
    before the guard existed, a 600-level document ended a debug test binary with
    `fatal runtime error: stack overflow` (`SIGABRT`), and a browser stack is
    smaller than the 2 MiB that took. `expander.rs::MAX_HIERARCHY_DEPTH = 64`
    now raises a named `ParseException` instead, matching how the port already
    bounds the two recursive halves of the grammar (`MAX_EXPR_DEPTH`,
    `MAX_BLOCK_DEPTH = 64`). **Deliberate, and strictly safer than the
    reference**: the shipped library's deepest subsystem is depth 1, so the
    ceiling is 64× what any real model needs, and no document the Java accepts
    within that range is refused. Pinned from both sides by
    `component_robustness.rs::the_hierarchy_ceiling_holds_exactly_where_it_says`.
14. **Component parameter substitution is exponential in hierarchy depth, and
    this port's constant is ~10× the Java's.** Both engines substitute a
    parameter's *expression* into every occurrence, so a subsystem passing
    `k = k + k` to a child that uses `k` twice doubles the tree per level. Java's
    immutable AST nodes are shared by reference and form a DAG; the Rust `Expr`
    is an owned tree and is deep-cloned. Measured against the oracle: at depth 24
    both produce `y = 16777216`, the Java in a few seconds and this port in 65 s;
    at depth 28 the Java still solves; at depth 32 the Java dies with
    `OutOfMemoryError: Java heap space` and kills the process, where this port
    would merely be very slow. **Open, and a latent cliff rather than a live
    problem** — nothing in the 295-component library or the 361-fixture corpus
    exceeds depth 1. Closing it means structural sharing in `Expr` (a contract
    file) or memoising `substitute_params`. Pinned at depth 16 by
    `component_robustness.rs::parameter_substitution_stays_within_its_measured_exponential`.

Full detail: workflow output `wk1ueuu8a` findings list (items 1–12); Phase 6's
items are recorded in `docs/status-phase6.md`.

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
