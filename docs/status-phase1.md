# Status — Phases 0–2 complete, Phase 3 boundary wired

> **Historical.** This document records the state at the end of Phase 1–3 and
> is kept for its divergence ledger (below), which is maintained. For the
> current state read [`docs/status-phase12.md`](status-phase12.md) first, then
> [`docs/status-wave3-f7.md`](status-wave3-f7.md) and decision
> [D9](decisions/0009-rustprop-backend.md); the older per-phase status docs
> (11, 10, 9, 78, 7, 6, 5, 4) fill in the history. *(Corrected 2026-08-21:
> this pointer previously named `status-phase78.md` as current — four status
> documents and five decisions had landed since.)*

**Date:** 2026-07-30 · Workspace at the time: 644 tests green, clippy
`-D warnings` clean, `cargo fmt` clean, wasm release bundle **397 KiB raw /
175 KiB gzipped** (budget 2 MiB). Current numbers are in `status-phase78.md`.

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
Phase 7 closed none and **opened four (15–18)**; the Phase 7–8 robustness pass
closed none and **opened five (19–23)** — four of them ceilings the Java does
not have, added because the Java's *caller* has a guard this port has no
equivalent of. Their full write-up is
[`docs/status-phase78.md`](status-phase78.md#what-these-phases-did-not-deliver--ranked-honestly).

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
   the four critical/triple constants for **water, R134a and R1234yf**, from the
   `FRPHTAB1` artifacts `tools/table-gen` generates offline (decision D1). The
   measured error against CoolProp 8.0.0 is `1e-7…2e-4` relative; on the
   promoted fluid documents it is `6.4e-07…7.2e-05`. **Narrowed 2026-08-06 by
   [D7](decisions/0007-auxiliary-property-grids.md)**, which added a second
   artifact kind (`FRAUX1`, `tools/aux-gen`) covering three surfaces the `(P,h)`
   split geometry cannot carry: the **incompressible glycols**
   (`INCOMP::MEG[x]` / `INCOMP::MPG[x]`, exact in pressure, error `1.6e-5…1.3e-3`),
   **air transport** at `(P,T)`, and **transport on the saturation line**
   (`viscosity`/`conductivity`/`Cpmass` at `Q=0`/`Q=1`) for all three tabulated
   fluids — which is the only place the two-phase correlations ever ask for it.
   Still refused by name, never approximated: humid air (`HAPropsSI`),
   **single-phase** transport off the dome, `Cvmass`, `Z`, speed of sound,
   Prandtl, surface tension, supercritical states, non-glycol mixtures, and the
   remaining CoolProp fluids — including air *states* (it has a transport grid,
   not a `(P,h)` table, and now says exactly that). **Open and structural** —
   closing it fully still means shipping `coolprop.wasm` (D1 option A, still
   available). D7 also put the wasm bundle 273 KiB over its 3072 KiB budget;
   that breach was **closed 2026-08-06** by packing the linked artifacts
   (byte-plane shuffle + deflate in `crates/frees-core/build.rs`, inflated once
   at install time), which took the module to **3031.0 KiB — under budget by
   41 KiB** without dropping or coarsening a grid. Read D7's consequences
   section before adding another fluid: 41 KiB is less than one `.phtab`.
10. **Twenty parity fixtures compare at a declared tolerance, not `1e-9`.** A
    direct consequence of 9: no table-backed engine can match full-accuracy
    CoolProp goldens at `1e-9`. `fixtures/tolerances.json` relaxes the *numeric*
    tolerance for each, with its measured error and mechanism recorded;
    `display_names`, `block_count` and error classification stay exact for all
    of them. Guarded: a stale or unnecessary entry fails the gate. The two
    loosest are D7's — `ev-thermal-management` at `2e-3` (measured `8.951e-4`)
    and `sysdesign-ex11-liquid-cooling-loop` at `5e-4` (measured `1.310e-4`).
    The first is worth understanding before it is used as a precedent: its
    number is **not** the glycol grid's error but that error landing on
    `nuSinglePhase`'s 2300..4000 laminar↔turbulent blend at `Re = 2987`, where
    Nu sweeps 3.66 → ~30. The same grid grades the second fixture an order of
    magnitude tighter. **Open**; closed by the same move as 9.
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
    *(Re-examined 2026-08-22, Wave C3: the memoisation half of that sentence
    was the wrong prescription and is withdrawn. The cost is not repeated
    identical calls — it is materialising the `2^d`-node result as an owned
    tree, which no memo can avoid; `k = k + k` at depth `d` simply *is* that
    many owned nodes when `Expr` boxes its children. The only real closure is
    the other half — structural sharing (`Rc`) in `Expr` — which touches
    `ast.rs` and ripples through parser, eval and the CAS, so it needs its
    own decision record rather than a wave commit. Left open, still latent:
    depth 1 remains the library's and corpus's maximum.)*

### Opened by Phase 7 (2026-07-31)

15. **`complete_display_names` is `putIfAbsent`, not `put`.** Composing
    `base_display + suffix` for every array element downcased the `A[1,1]`
    spellings `emit_matrix` registers from a `LINEARIZE` header. Element names
    are otherwise always absent — the parser registers only an array's base
    spelling — so every pre-existing fixture is unaffected, confirmed by the
    390-fixture replay. **Deliberate.**
16. **No `changeInVariables` in `relaxed_ode_settings`.** The Java clamps
    `Math.max(base.changeInVariables(), 1e-9)`; this port's Newton has no such
    knob (its stop rule is the residual only), so only `rel_tolerance` is
    transcribed. **Open, and invisible to the corpus.**
17. **The accessor bridge is dropped before the final `solveDynamicSystems`.**
    The Java reaches there with its thread-local still installed. Re-integrating
    from scratch against the final values is the safer reading — a cached table
    from a Newton iterate must not become the published trajectory — and no
    fixture distinguishes the two. **Deliberate.**
18. **`ode_tables` absent from a golden means "not a claim", not "no tables".**
    A pre-Phase-7 golden asserts nothing about trajectories. A Rust engine that
    *produces* a table where the golden is silent fails with a "re-dump this
    fixture" message, so absence can never hide a transient. **Deliberate.**

### Opened by the Phase 7–8 robustness pass (2026-07-31)

Four are new ceilings the Java does not have. All four exist because the Java's
*caller* has a guard this port has no equivalent of — `SolveController` /
`OptimizeController` clamp their request DTOs before core ever sees them, and
`OdeProblem` carries a `deadlineNanos` this port cannot check (`ode/problem.rs`,
*No clock*). Each was chosen as the Java's own upstream number where one exists,
so no in-range request behaves differently. Each is pinned from both sides by
`crates/frees-core/tests/dynamics_robustness.rs`.

19. **`ode::problem::MAX_OUTPUT_SAMPLES = 100_000` bounds a `DYNAMIC` block's
    output rows.** `OdeIntegrator.integrate` sizes `double[sampleCount]` plus a
    `double[dimension]` per sample straight from the header's `points`, with no
    ceiling. Measured: `points = 1e9` **aborted the process** —
    `memory allocation of 8000000000 bytes failed` — before taking one step. On
    the JVM that is an `OutOfMemoryError` the web layer turns into a 500; under
    the wasm `panic = "abort"` profile it kills the worker, which no `Result`
    can catch. The value is `MAX_RANGE_ELEMENTS`, the ceiling the parser already
    applies to a materialised `PARAMETRIC` sweep. **Deliberate, and strictly
    safer than the reference**: the largest `points` in the 500-document corpus
    is 1 201, so the ceiling is 83× what any real model asks for.
20. **`ode::integrator::MAX_CONSECUTIVE_SET_RESTARTS = 1_000` bounds a
    non-progressing `set`-event loop.** A `set` whose assigned value re-arms its
    own crossing turns the time loop into a restart loop that advances `t` by
    ~0 while spending 60 RHS evaluations per pass bisecting. `MAX_STEPS` does
    bound it, but only after 10^6 passes: measured, two such documents were
    still running when killed at a 45 s CPU limit, against 182 s for the
    stiff-on-explicit case that reaches the same ceiling without bisecting.
    **The guard is a rate test, not a firing count** — the first cut counted
    consecutive restarts and wrongly refused a legitimate 500 s sawtooth, whose
    adaptive step outgrows its 0.1-wide switching period so that *every* step
    brackets a crossing. What decides it is whether the window advanced time
    fast enough to finish inside the remaining step budget; the margin between
    the two cases is ~4×10^3 projected steps against ~9×10^10. **Deliberate.**
21. **A non-finite time span is refused; the Java silently answers it.**
    `tf = inf` passes `tf <= t0` (it is greater than any `t0`) and `tf = NaN`
    passes it too (every NaN comparison is false). `span` and `min_step` then go
    non-finite, the loop condition is false on the first pass so nothing is
    integrated, and `integrate` publishes a full-height table anyway. Measured
    before the screen: **200 rows of `[NaN, inf, inf, …]`, returned as a
    trajectory** — the only *silent wrong answer* this audit found, and the
    worst of the three failure modes. Unreachable from a document (the parser's
    `signedNumber` admits no infinite literal) but `OdeProblem` is public API.
    **Deliberate.**
22. **The initial *state* is checked for finiteness, not just the initial
    derivative.** The Java checks only the derivative. A NaN `y0` with a finite
    RHS poisons `scale = atol + rtol*|y|`, so every error test and every
    `h_use < min_step` comparison is false, nothing is ever rejected, and the
    run burns all 10^6 steps before blaming stiffness. One comparison up front
    is both faster and true. **Deliberate; strictly a better diagnostic.**
23. **`montecarlo::run` and `pareto::optimize_multi` no longer pre-allocate on
    an untrusted count.** `new ArrayList<>(sampleCount)` and the NSGA-II
    population vector are transcribed faithfully, but the Java's controllers
    clamp first — `frees.solver.max-mc-samples` defaults to **1000** and
    `clampPositive(populationSize, 40, 200)` caps the population at **200**.
    Measured: `samples = 1e9` reserved **56 GB** and aborted the process, before
    the deadline predicate was consulted once. Both now bound the reservation at
    the controller's own number. **Deliberate.** *Note this is a symptom of a
    larger gap, not just a bug: none of `analysis/` is reachable from a document
    or the wasm boundary yet, so its validation has no upstream owner.*
    *(Update 2026-08-22, Waves B1/B2: two of the surfaces now have their owner
    — `frees-wasm/src/analysis.rs` validates the table sweep (row cap,
    cooperative deadline) and the Monte Carlo run (the 2–1000 sample cap with
    the Java's verbatim message, the 120 s truncating budget). The
    optimizer/NSGA-II/curve-fit/param-fit validations still await their
    exports.)* *(Wave B3, same day: they arrived — all six analysis surfaces
    with REST endpoints now validate at `frees-wasm/src/analysis.rs`.)*

### Opened by the Phase 9 robustness pass (2026-08-01)

Both found by the adversarial sweep over the CAS and the control suite, and
pinned by `crates/frees-core/tests/cas_control_robustness.rs`. Full context in
[`docs/status-phase9.md`](status-phase9.md).

24. ~~**`balreal` returns a valid balanced realisation whose second state
    carries the opposite sign from the oracle's.**~~ **Closed 2026-08-21
    (Wave A2).** The convention proved not statable as a normalisation — the
    `linalg-full-svd` golden's V column 2 has its largest component
    *negative*, contradicting any make-positive rule — so, per this item's own
    closing sentence ("a convention is transcribed rather than invented"),
    `linalg::svd`'s one-sided Jacobi kernel was replaced with a line-faithful
    transcription of Commons Math 3.6.1's JAMA-derived
    `SingularValueDecomposition`, Householder reflector signs and all. All
    six sign-held fixtures (this one, `linalg-full-svd`,
    `multiout-svd-discard-with-tilde`, `ctldesign-balreal-invariants-integration`,
    `ctldesign-bare-matrix-names-into-control-calls-resolve-shapes`, `-2`)
    promoted at the corpus default `1e-9`; corpus 722 → 728. The original
    diagnosis, kept because its sign-flip mechanics are what made the fix
    checkable: re-checking
    `fixtures/corpus-pending/corpus/estimator-gramian-balreal.frees` against its
    golden: `L` (the Kalman gain), `Wc` and `Wo` all match to better than
    `1e-9`, and exactly four entries mismatch — `Ab[1,2]`, `Ab[2,1]`,
    `Bb[2,1]`, `Cb[1,2]` — each with the right magnitude and the wrong sign.
    That is a signature, not a coincidence: `T = Lc·V·S^{-1/2}` and
    `T⁻¹ = S^{-1/2}·Uᵀ·Loᵀ`, so flipping the joint sign of column 2 of the SVD's
    `U` and `V` flips column 2 of `T` and row 2 of `T⁻¹`, which flips exactly
    `Ab`'s off-diagonal, `Bb`'s second row and `Cb`'s second column. An SVD's
    singular vectors are determined only up to that joint sign; Commons Math and
    `linalg::svd` choose differently, and neither is wrong. **Open, not
    deliberate** — a fix means matching Commons Math's sign output, which cannot
    be inferred from one data point, and the parity rule here is that a
    convention is transcribed rather than invented. The invariants that matter
    *are* asserted: the realisation is internally balanced (`Wc = Wo = diag(σ)`)
    and the transfer function is unchanged by the balancing.
    (`control/design.rs::balreal`,
    `balreal_is_internally_balanced_even_though_its_state_signs_differ`)
25. **The CAS's `MAX_POW = 64` silently turns "cannot factor" into "did not
    factor".** `factor(x^100 + 1)` returns `1+x^100` — correct as a value,
    unfactored as an answer — because `x^100` interned as one opaque generator
    rather than being expanded. `factor(x^50 + 1)` at the same moment returns a
    genuine factorisation. Symja factors both. There is no channel that
    distinguishes *"proved irreducible"* from *"declined above the exponent
    ceiling"*, so a user cannot tell which they were given. `Apart` has the same
    shape at `MAX_APART_DEGREE = 64`: over the ceiling it returns the input
    unchanged rather than saying it declined. ~~**Open.**~~ **Closed
    2026-08-22 (Wave C5).** The ceilings themselves
    are right — they are what stops `(x+1)^100000` being a denial of service,
    and wasm has no timeout to fall back on — but "declined" needs to be
    sayable. It now is: the two gates record a thread-local note that
    `engine::apply_expr` drains into `CasResult::note`, and the REPL prints
    it under the value (`(declined: an exponent of 100 exceeds the ^64
    expansion ceiling …)`). The value line itself is byte-identical to
    before — the note is a separate channel, so no parity surface moves.
    No `diag.rs` change was needed, honouring the contract-file rule.
    (`cas/ops.rs::record_ceiling_note`, pinned by
    `cas_control_robustness::a_ceiling_decline_carries_a_note_instead_of_silence`)

### Opened by Phase 10 (2026-08-01)

The measurement surface (`crates/frees-core/src/measurement/`,
`crates/frees-wasm/src/measurement.rs`). Pinned by
`crates/frees-core/tests/measurement_parity.rs` and
`crates/frees-core/tests/measurement_robustness.rs`. Full context in
[`docs/status-phase10.md`](status-phase10.md).

26. **MDF4 format coverage narrows, and the `mdf-sidecar` rung has no
    successor.** The Java ran `Mf4Parser` (mdf4j, in-process) →
    `FallbackMeasurementParser` → the Python **`mdf-sidecar`** (asammdf), whose
    own docstring says it exists for *"DZ-compressed data blocks
    (deflate/ZSTD/LZ4), the norm for OEM recordings"*. In the browser there is
    no second process to fall back to. Against `mf4-rs` 3.6 this port **loses**
    deflate `##DZ` (which mdf4j read), and loses ZSTD/LZ4 `##DZ`, VLSD string
    storage and unsorted/multi-group data groups (which the sidecar read); it
    **gains** rational, algebraic and table-lookup conversions over mdf4j's
    identity + linear. `mf4-rs` has no decompressor anywhere in its dependency
    tree — verified, its manifest carries no `flate`/`zstd`/`lz4`/`miniz`/`zlib`
    edge — so closing this means one to three new crates in a bundle with 128 KiB
    of headroom. **Deliberate, and the single largest functional regression in
    the port so far.** The refusal happens at `open`, names the remedy
    (re-export uncompressed) and does not let the user discover it one channel
    at a time. Also narrowed, and *unproven* rather than deliberate: virtual
    master channels (`cn_type` 3) are refused, and mdf4j's opaque
    `isTimeMaster()` does not reveal whether the Java accepted them.
    (`measurement/mdf4.rs::open`, `check_storage_chain`)
27. **`movavg` recovers once a non-finite sample leaves its window; the Java's
    does not.** `TimeSeriesEvaluator.movavg`'s running sum is a one-way door —
    one `±∞` sample, or two large finite ones, poisons the accumulator and every
    later point comes back `NaN`. Measured on a 500 000-point channel with one
    `+∞` at the midpoint, that is 245 998 fabricated gaps over data that is
    fine. This port tracks the window's `±∞` population with the same
    add-on-entry/subtract-on-exit bookkeeping as `count` and recomputes once the
    population reaches zero. **Deliberate**: a gap in a measurement tool is a
    claim about the instrument, and inventing one is worse than a parity
    difference. Everything adjacent is *not* changed — `NaN` samples are still
    skipped rather than propagated, and `integral` still propagates `±∞` the
    Java way, both pinned. (`measurement/calc.rs::movavg`,
    `a_moving_average_recovers_over_a_window_of_realistic_width`)
28. **Ragged and degenerate inputs answer where the Java throws.** Three sites,
    one reason: the wasm release profile is `panic = "abort"`, so a throw takes
    the whole module — and with it the recording the user was promised would
    never leave the tab. (a) Mismatched `t`/`v` lengths: Java throws
    `ArrayIndexOutOfBoundsException`, this port treats the series as its common
    prefix. (b) A degenerate or out-of-range `min_max` window: Java throws, and
    for `i1 < i0` its midpoint index is `(-1) >>> 1` = 2³¹−1; here the range is
    clamped into the data and a degenerate range returns an empty `Envelope`.
    (c) `raster::fixed` over an infinite span: Java's
    `(long) Math.floor(inf) + 1` overflows to `Long.MIN_VALUE` and it silently
    returns an **empty** raster (and `fixed(inf, inf, 1, 100)` returns
    `[Infinity]`); here both are refused. **Deliberate.** One preserved blind
    spot, kept so all three implementations stay bucket-for-bucket identical: a
    decimation bucket whose samples are all `+∞` reports as a gap, because `+∞`
    is the "nothing seen" sentinel — Java and the frontend's `decimate.ts` share
    this, and it is pinned rather than fixed.
    (`measurement/series.rs::at`, `measurement/decimate.rs::min_max`,
    `measurement/raster.rs::fixed`)
29. **Six structural ceilings exist here that have no Java analogue, because the
    Java's equivalents were a server and a JVM.** mdf4j memory-mapped the file
    and streamed records (its own spike test asserts <60 MB of heap on a 100 MB
    file); a browser has bytes, not a file, so the whole recording plus its
    decoded `f64` columns is held. `MAX_RECORDS` (16.7 M samples = 256 MiB per
    channel), `MAX_BLOCKS`, `RETAINED_BYTES_BUDGET` (512 MiB across all open
    files, LRU-evicted), `MAX_FORMULA_NODES` (1024),
    `MAX_SYNTHETIC_SAMPLES` and `MAX_INPUT_COLUMN_SAMPLES` (128 MiB each)
    replace them. The formula-width bound is the sharpest divergence of the six:
    `TimeSeriesEvaluator` has none, and a shallow enormous formula measured
    **51 s** to evaluate over a million-point raster and **781 MB** of synthetic
    columns from 14 kB of text. **Deliberate.** *Note the shape, because it is
    the same one as item 23: the Java's input validation lived in its
    controllers, and a port that deletes the controllers must put it somewhere.*
    Three of these six were also found to be **stated in the wrong unit** — a
    file could satisfy them and still cost unbounded time or memory — and that
    is what the robustness sweep fixed, not the ceilings themselves.
30. **The boundary refuses three things the Java quietly accepted.** An
    unrecognised interpolation mode is refused by name rather than defaulted to
    `linear`; two inputs whose names collide only by case are refused rather
    than one silently dropping the other (variable names are case-insensitive,
    so `X` and `x` are one binding); and a text-declared channel stays text
    rather than being sniffed into a number. An inverted window range returns an
    empty window rather than throwing, which is the item-28 reason again.
    **Deliberate.** (`crates/frees-wasm/src/measurement.rs`)

**One divergence *closed* by this pass, engine-wide and worth flagging because
it changes documents too.** `^` was C's `pow`, which answers `1` for
`pow(1, NaN)` and `pow(±1, ±∞)` where Java's `Math.pow` answers `NaN`. Found in
the calc tree, fixed there, then found *again* inside function-call arguments —
which are not compiled and go to the document evaluator instead. The second fix
is in `eval.rs::apply_binop`, so **every** frees document now answers `NaN` for
`1^NaN` and `1^inf` where it previously answered `1`, which is what
`ast/Evaluator` has always done. No corpus fixture changed. Two sites, one rule;
both pinned in `measurement_parity.rs`.

Full detail: workflow output `wk1ueuu8a` findings list (items 1–12); Phase 6's
items are recorded in `docs/status-phase6.md`, Phase 7's in
`docs/status-phase7.md`, items 19–23 in `docs/status-phase78.md`, items
24–25 in `docs/status-phase9.md`, and items 26–30 in `docs/status-phase10.md`.

### Opened by Phase 11 (2026-08-05)

The browser-native product layer. These diverge from the **vendored frontend
and its deployment**, not from the Java engine — engine behaviour is untouched
this phase. Full context in [`docs/status-phase11.md`](status-phase11.md).

31. **Workspace autosave is now dual-written.** Upstream autosaves to one
    `localStorage` key (`frees.project`) and silently stops updating past the
    ~5 MB quota. This port mirrors every autosave into IndexedDB
    (`projectStore.ts`, decision D4) and, when the mirror is strictly newer at
    boot, *offers* — never forces — a restore. Upstream has no equivalent and
    no recovery from a quota-dead autosave.

32. **The web deployment is static-only.** The nginx `/api` proxy blocks, the
    `limit_req` rate limiter and the `real_ip` trust machinery are deleted
    from `web/nginx.conf.template` and `web/Dockerfile` — they guarded a
    remote compute tier this build does not have. A hybrid deploy re-enters
    through `VITE_API_BASE` (the adapter in `api.ts` is kept, unwired), and
    would need `connect-src` widened in `security-headers.conf` for a
    cross-origin backend.

33. **The app is an installable PWA with a full-precache service worker.**
    Upstream ships a manifest with `icons: []` (not installable) and no
    service worker. This port precaches the entire built app (~30 MB,
    including the wasm engine and the Plotly/spreadsheet lazy chunks) under a
    prompt-style update flow. The trade is deliberate: first visit downloads
    the whole tool; every later session — including offline — costs nothing.

### Opened by Phase 12 (2026-08-05)

The hardening pass: corpus growth to 701, property-based fuzzing, benchmarks.
Full context in [`docs/status-phase12.md`](status-phase12.md).

34. ~~**`CALL eigenvalues` / `CALL eigen` are not wired.**~~ **Closed
    2026-08-21 (Wave A1).** The Java routes both through Commons Math's
    eigen-decomposition; this port refused them explicitly until
    `parser/expand.rs::flatten_eigen` (a transcription of
    `EquationParser.flattenEigen`/`emitEigenvectors`) and the
    `eigen$val|re|im|vec$…` kernels in `linalg::eval_intrinsic` (the
    `Evaluator.evalEigen` contract: **ascending** (real, imag) sort on top of
    the decomposition, unit-2-norm eigenvectors with the largest-magnitude
    component made positive via a strictly-greater tie-break) landed. The
    three `eqsys-*eigen*` documents promoted at the corpus default `1e-9`
    with no tolerance entry; corpus 719 → 722.

35. **An explicit `~` discard in destructuring leaks Java's global sink
    counter.** `[whole, ~] = DivMod(17, 5)` records `~ignored~N` in the
    oracle's `display_names` with a JVM-batch-global `N` — unreproducible
    across dump runs, so such fixtures can never be frozen (this port's
    per-document counter is better behaved and will never match). Oracle-side
    hazard, recorded in `fixtures/README.md`; not a Rust defect.

36. **The dead-end analysis UI is clipped (decision D5, 2026-08-05).** The
    Min/Max, Curve Fit, PID Tuner, Monte Carlo and Parameter Estimation
    modals and the PDF/EPS plot exports are removed from the product rather
    than shipped as `NOT_IN_BROWSER_ENGINE` apologies. The upstream frontend
    keeps them (its backend answers); this build's UI promises only what its
    engine does. The `api.ts` stubs, the pid helpers and the Phase 8
    `analysis/` module remain as the wiring seam, with acceptance fixtures
    staged in `corpus-pending/`.

37. **MDF4 is removed outright (decision D6, 2026-08-05).** The `.mf4`
    reader, the `mf4-rs` dependency (and with it `meval` → `nom 1.2.4`, the
    workspace's only future-incompat warning — now zero), the boundary's
    opened-file registry and three of its four measurement exports, and the
    analyzer's remote-source path are gone. The Data Analyzer is CSV-only;
    `measurement_calc` survives, stateless, inputs inline. Three of Phase
    10's fifteen defect regressions and both MDF4 fuzz properties left with
    the surface they defended. Divergence from the Java baseline is now
    total for this format rather than partial: where Phase 10 read a subset,
    this build reads none, and says so in the analyzer's Help page.

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

*(Corrected 2026-08-21: everything this section listed shipped long ago — the
browser vertical slice in Phase 3, corpus growth through Phases 4–12 and the
Phase-12 harvest, and the Phase 4 differentiator/arrays/complex/procedural
work. The list is kept only so old links into it do not dangle; do not plan
from it. Current open work is inventoried in
[`docs/status-phase12.md`](status-phase12.md)'s "did not deliver" list and the
ledger above.)*
