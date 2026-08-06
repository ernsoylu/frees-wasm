# Phase 12 — parity at scale, performance, hardening

**Read this after [`status-phase11.md`](status-phase11.md).** Phase 12 is the
last row of PLAN.md's plan: grow the parity corpus toward the full Java test
surface, benchmark against the JVM engine, fuzz the hostile-input surfaces,
break the bundle down by name, and close the panic/memory story on the wasm
boundary. No product feature ships in this phase; what ships is **evidence**.

The engine changed in exactly zero lines this phase. Everything below is
tests, fixtures, benches, tooling and documentation — which is why the corpus
could grow 32 % with the parity gate green on the first full replay.

---

## Gate numbers, all raw

| Gate | Result |
|---|---|
| `cargo test --release --workspace` | **3162 passed, 0 failed, 6 ignored** across 26 suites, exit 0. The delta reconciles exactly: 3155 (Phase 11) + 7 fuzz properties = 3162; nothing else moved |
| `cargo test -p frees-core --test parity` | `golden_corpus_parity` **ok** — all **701** fixtures match the Java oracle, 165 s (531 took 41 s; the growth is real documents, not padding). *Post-phase: 702 since 2026-08-06 — `heisler-transient` promoted by the `StringVariables` port.* |
| `cargo test -p frees-core --test fuzz_properties` | **7 properties pass** at CI case counts; a `PROPTEST_CASES=4096` soak (≈15 500 generated inputs) also passes |
| `cargo clippy` (native + wasm32, `--all-targets`) | exit 0 on both — including the new bench/fuzz targets, which are cfg-gated off wasm32 (proptest's `getrandom` refuses that target; found by running the gate, not by reading docs) |
| `cargo fmt --all --check` | exit 0 |
| `./node_modules/.bin/vitest run` | **40 files, 394 passed** (Phase 11: 39/388; +1 file/+6 is `engineClient.test.ts`) |
| `cargo bench -p frees-core --bench solve_bench` | five benches, table below |
| wasm bundle | **untouched at 2944 KiB** — the phase adds only dev-dependencies, and `[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]` makes that a checked property, not an assumption |

New dependencies: **zero runtime**, two dev and native-only — `proptest` 1.x
(minus default features) and `criterion` 0.5 (minus default features).

---

## 1. The corpus: 531 → 701, and what the "1,237" really was

PLAN.md's Phase 12 row says "full 1,237-test corpus replay". Measured against
today's reference checkout, **that number is stale**: `backend/core` now has
**184 test files / 1,179 `@Test` methods** (some were consolidated upstream
since PLAN.md was written). More importantly, the right unit is not tests but
*documents*: only **~625** of those tests carry a `.frees` document that can
become a corpus fixture; the rest are unit tests of Java internals
(`DifferentiatorTest` alone is 65 tests of a class this port re-implements
behind the same fixtures).

The harvest (`tools/harvest-java-tests/harvest.py`, run against the live
oracle with the **macOS CoolProp dylib** — see below):

| Stage | Count |
|---|---|
| candidates extracted from 13 Java test classes | **212** |
| dropped at golden review (settings-bearing tests, `String.format` templates, `~ignored~N[` array sinks, one scalar-`~` leak) | 21 |
| kept and replayed | **191** |
| **passed parity at default tolerance, promoted** | **170** |
| failed, classified, staged in `corpus-pending/` | 21 |

The 21 failures divide cleanly — and the division is the finding:

* **6 are new witnesses of already-documented divergences**, not new bugs:
  five hit the `linalg::svd` column-sign convention (same mechanism as the
  recorded `estimator-gramian-balreal` hold; sign-only flips, invariants
  identical), one hits the pre-Newton property-probe divergence (same as
  `ev-battery-cooling-pid`).
* **12 are unported features**: `HAPropsSI` humid air (5), D1 untabulated
  fluids `Air`/`INCOMP::MEG` (4), and — the one genuinely new gap this
  harvest found — **`CALL eigenvalues`/`eigen` is not wired** (3 documents;
  ledger item 34).
* **3 are oracle/comparison artifacts**: two D1 table-accuracy cases inside
  the documented band (worst 7.9e-8), promotable only with measured
  `tolerances.json` entries that were deliberately not added in bulk; one
  asymptotic-FP case where a column decays to machine zero and the engines
  sit one 4.5e-12 quantum apart.

Zero unexplained failures. The full triage, including the five-witness sign
table, is in `fixtures/README.md`'s Phase 12 re-check note.

**The macOS oracle.** This phase ran the Java oracle on a Mac for the first
time. Four traps, all now recorded in the README/status docs: `FREES_HOME`
defaults to a Linux path; the core jar was unbuilt; **the vendored
`libCoolProp.so` is a Linux ELF that JNA silently fails to load** — the fix
is `COOLPROP_LIBRARY=/usr/local/lib/libCoolProp.dylib`, verified against the
known ground truth (`Enthalpy(Water, 300 K, 101 325 Pa)` →
`112654.8996546125` vs the Linux oracle's `…4505` — a ~2e-12 CoolProp-build
difference, three orders below the parity tolerance); and SUNDIALS is not
installed, so `fixtures/dae-oracle.json` is **frozen on this machine**.

Also found and recorded (README + ledger item 35): an explicit `~` discard in
destructuring (`[whole, ~] = DivMod(…)`) leaks Java's JVM-global sink counter
into `display_names`, so such fixtures can never be frozen — a correction to
the README's previous "scalar sinks are safe" rule, which covers only
omitted-trailing sinks.

---

## 2. Property-based fuzzing (`tests/fuzz_properties.rs`)

The hand-written robustness suites (565 tests) encode chosen adversarial
cases; this file generates them. Seven properties over four surfaces:

* **Unstructured**: arbitrary unicode; arbitrary bytes lossily decoded.
* **Structure-aware documents**: a grammar strategy emitting parseable
  documents (unit annotations, FOR/GUESS blocks, function calls, matrices,
  builtin-colliding identifiers, `1e309`-class numbers) — these reach the
  blocker and Newton, where random bytes never arrive. Plus a determinism
  property: solving twice gives bitwise-identical results.
* **Unit annotations**: arbitrary printable text inside `[…]`.
* **MDF4**: byte-splicing and **aligned 64-bit link forging** inside the
  genuine asammdf recording, so the block-graph pre-flight engages with
  plausible structure instead of refusing at the header.

All seven pass at CI counts and at an 8× soak. That is the *expected* result
— it is the same contract the 565 hand-written tests enforce, now checked
over ~15 500 generated inputs per soak — and the honest reading is coverage
confirmation, not proof of absence: proptest is not coverage-guided. The
file's header requires any future minimized counterexample to be promoted
into the matching hand-written suite as a named regression.

---

## 3. Benchmarks: the port vs the JVM oracle

`benches/solve_bench.rs` (criterion, end-to-end through public `solve`) vs
the JVM driven through the golden dumper — same five documents, 50 copies
each to amortize startup, 2.87 s baseline subtracted, this machine:

| document | what it exercises | Rust (native) | JVM oracle | ratio |
|---|---|---|---|---|
| `scalar_two_block` | parse → block → Newton | **147 µs** | ~8.2 ms | ~56× |
| `rankine_cycle` | real-fluid property tables | **771 µs** | ~49 ms | ~63× |
| `component_mvem` | library expansion + mixed system | **776 µs** | ~8.8 ms | ~11× |
| `transient_dyn` | DYNAMIC → ODE integrator | **133 ms** | ~132 ms | **~1.0×** |
| `control_lqr` | state space, LQR, CAS helpers | **2.97 ms** | ~22 ms | ~7× |

Caveats, stated rather than buried: the JVM numbers include the dumper's
JSON serialization and only partial JIT warmup (50 iterations), so treat the
ratios as **user-visible-latency** comparisons, not microarchitectural
claims. The transient row is the honest anchor: ~1.0×, because the
integrator was ported line-for-line and both engines spend the time in the
same algorithm — which is exactly what D1/PLAN predicted (the port wins on
call overhead and property-table interpolation, not on numerics). The real
product win remains the term that is *absent* from the table: the network
round-trip every JVM solve pays in production and the wasm solve does not.

Not measured: the same benches inside the browser. Phase 9's in-browser CAS
timings (103 ms `Expand` in the shipped REPL vs 27 ms native) suggest a
2–4× wasm-vs-native factor; measuring it properly is future work.

---

## 4. The worker-death path, finally tested

`web/src/wasm/engineClient.test.ts` drives the **real** engineClient
singleton with a fake Worker — every prior suite mocked the module away. Six
tests pin the product's only recovery mechanism under `panic = "abort"`:
correlation by id (answered out of order), a worker `error` mid-flight
rejects **all** pending promises and terminates the corpse, **the next call
respawns**, `messageerror` fails over identically, a stray-id response is
ignored, an `{ok:false}` response rejects one call while keeping the worker,
and measurement bytes ride the transfer list (`transfer === [buffer]`), not
a structured clone.

On the "memory ceiling" row of PLAN's Phase 12 scope: the survey confirmed
`RETAINED_BYTES_BUDGET` (512 MiB, measurement) is the only byte-denominated
guard, and that the sweep/Monte-Carlo paths PLAN wanted guarded are **still
unreachable from the boundary** (Phase 8's gap). There is nothing to guard
until they are wired; writing a guard for dead code would be theater. The
structural bounds (18+ ceilings, catalogued in the survey and status docs)
plus worker respawn are the shipped memory story.

---

## 5. Bundle breakdown, by name

The tree finally has a **named, per-module breakdown** — something no
previous phase produced (`status-phase10.md` measured by differential build
because the shipped artifact strips its name section). Method: `twiggy` over
the **pre-bindgen** release artifact
(`target/wasm32-unknown-unknown/release/frees_wasm.wasm`, 4,458,455 B — the
one build that still carries names; the shipped 2944 KiB is this minus the
name section, wasm-bindgen processing and `wasm-opt -Oz`, so treat the
proportions as ranking, not shipping bytes):

| Component | bytes | % of named artifact |
|---|---|---|
| name section (stripped from the shipped artifact) | 1 012 611 | 22.7 % |
| `.rodata` — the two `(P,h)` property tables (~526 KB) + the embedded component library (~122 KB) + diagnostic strings | 890 222 | 20.0 % |
| `frees_core::cas` | 297 571 | 6.7 % |
| `frees_core::parser` | 291 925 | 6.5 % |
| `frees_core::control` | 231 626 | 5.2 % |
| `frees_core::eval` | 176 543 | 4.0 % |
| `core` (Rust language runtime) | 150 862 | 3.4 % |
| `frees_wasm` (the boundary itself) | 146 351 | 3.3 % |
| `frees_core::props` (code; tables are in `.rodata`) | 122 652 | 2.8 % |
| `alloc` | 115 840 | 2.6 % |
| `frees_core::components` (code; library text in `.rodata`) | 110 358 | 2.5 % |
| `frees_core::ode` | 99 816 | 2.2 % |
| `num_bigint` (CAS exact arithmetic) | 90 668 | 2.0 % |
| `frees_core::engine` | 90 213 | 2.0 % |
| `frees_core::ast` | 65 242 | 1.5 % |
| `mf4_rs` | 55 798 | 1.3 % |
| `frees_core::measurement` (code) | 54 368 | 1.2 % |
| `linalg` / `units` / `serde_json` / `libm` / `analysis` / `solver` | 48 590 / 48 298 / 44 214 / 42 385 / 41 754 / 31 990 | ~0.7 % each |
| everything else (petgraph, num_rational, dlmalloc, std, small modules) | ~57 000 + tail | — |

Reading it: **the two known debts are confirmed as the two biggest levers** —
`.rodata`'s property tables dominate shippable data exactly as the CI comment
claims, and no single code module exceeds ~7 %, so code-splitting buys little
until the CAS/control pair (~12 % together, REPL-only surfaces) is split with
the data. On the shipped artifact, `twiggy dominators` names the exports:
`solve` retains 215.8 KB, `repl_evaluate` 145.6 KB, `measurement_calc`
57.9 KB, `check` 23.6 KB, and the indirect-call `table[0]` 337.9 KB.

The budget gate is untouched: 2944 KiB raw against 3072 KiB, 95.8 %, and the
two recorded debts (fetch the property tables, split the engine) remain the
only real levers. This phase added nothing to the bundle and proved it by
gating the new dev-dependencies off the wasm target in the manifest.

---

## What Phase 12 did **not** deliver — ranked

1. **The corpus is 701, not "everything".** Remaining Java documents are the
   shapes the harvester cannot represent: tests passing extra `solve(...)`
   arguments (complex mode, `ProcDef` function tables — the biggest block),
   `String.format` templates, cross-file constants. Plus the two untouched
   growth sources: `../frEES/frontend/src/docs/*.md` snippets and the 295
   component library sources. A follow-up needs harvester features, not more
   sweat.
2. **`CALL eigenvalues`/`eigen` found unwired and left unwired.** This was a
   hardening phase; wiring a new CALL belongs with the Phase 8 backlog. The
   three documents wait in `corpus-pending/` as the acceptance test.
3. **No browser-side benchmark.** Native-vs-JVM only; the wasm factor is
   inferred from Phase 9's REPL timings, not measured on these documents.
4. **The DAE surface is still un-fuzzed at API level**, and the SUNDIALS
   oracle cannot be regenerated on this machine (not installed) — its
   `ORACLE_*` constants are effectively frozen until someone installs
   SUNDIALS ≥ 6 with KLU.
5. **Benchmarks are not a CI gate.** They run locally (`cargo bench`); no
   regression threshold exists. Deliberate for now — criterion noise on
   shared CI runners produces flaky gates — but it means a performance
   regression is caught by a human re-running the table, or not at all.
6. **Fuzzing is not coverage-guided.** proptest generates blind; a
   `cargo-fuzz`/libFuzzer target with the same oracles would explore deeper.
   The repo still has no nightly toolchain requirement, and adding one for
   fuzzing was judged not worth it this phase.
7. **Carried forward from Phase 11**: no CI job runs the service worker
   offline; the remote-fallback adapter stays unwired; the precache-size
   opt-out was explicitly deprioritized (features may be clipped instead).

---

## Divergences opened by this pass

Ledger items **34** (`CALL eigenvalues`/`eigen` unwired) and **35** (the
destructuring-`~` sink-counter leak, an oracle-side authoring hazard) in
[`status-phase1.md`](status-phase1.md#opened-by-phase-12-2026-08-05). The six
new failing witnesses of items already on the ledger are recorded against
those items in `fixtures/README.md`, not double-counted here.
