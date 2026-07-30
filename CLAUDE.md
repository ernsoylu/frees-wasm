# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository status

**Phases 0–4 are implemented** — the Phase-3 wasm boundary is wired and Phase 4
(differentiator, matrix, complex, procedural, tables, integrals, kernels, latex,
solver retry ladder) is complete. A Rust workspace ports the frees engine to
WebAssembly. **1,341 tests** green, including a **204/204** golden-corpus parity
replay against the real Java engine; wasm 1147.7 KiB raw / 436.1 KiB gzipped
(budget 2048 KiB). Solve, check and the language reference all run in-browser
with **zero `/api/` traffic**. Phase 5 (properties/CoolProp) is next.

| Document | Contents |
|---|---|
| [`docs/status-phase4.md`](docs/status-phase4.md) | **Read first.** What Phase 4 delivers per area, the true gate numbers, fixture counts, and the honest ranked list of what it did *not* deliver |
| [`docs/status-phase1.md`](docs/status-phase1.md) | The maintained divergence ledger (items closed are struck through with a date), plus the Phase 0–3 inventory |
| [`PLAN.md`](PLAN.md) | The phased plan: architecture, decisions, parity strategy, 13 phases, risks |
| [`docs/dependency-map.md`](docs/dependency-map.md) | Every Java/native dependency → Rust replacement |
| [`docs/feature-inventory.md`](docs/feature-inventory.md) | All 134 `backend/core` files mapped to features and phases |
| [`docs/decisions/`](docs/decisions/) | D2 (wasm32-unknown-unknown + wasm-pack), D3 (worker pool, no COOP/COEP) |
| [`fixtures/README.md`](fixtures/README.md) | The parity harness: corpus, golden fixtures, tolerance policy, oracle-established ground truths |

## Build and test

```bash
export PATH="$HOME/.cargo/bin:$PATH"   # toolchain is rustup-installed; distro rustc is stale
cargo test --release --workspace       # all tests incl. the parity replay
                                       # (--release: the replay solves 204 documents)
cargo test -p frees-core --test parity # golden-corpus parity only
cargo clippy --workspace --all-targets -- -D warnings   # CI gate
cargo clippy --workspace --target wasm32-unknown-unknown --all-targets -- -D warnings
cargo fmt --all --check                                 # CI gate
wasm-pack build crates/frees-wasm --release --target web --out-dir ../../web/src/wasm/pkg
tools/golden-dumper/run.sh             # regenerate golden fixtures from the Java oracle
```

> **`rtk` condenses these commands' output.** `cargo test` comes back as a
> one-line summary with no per-suite results, and clippy/fmt warnings are
> swallowed entirely. To see real output, call the binary by absolute path
> (`"$HOME/.cargo/bin/cargo"`, `./node_modules/.bin/vitest`) and redirect to a
> file.

## Workspace layout

- `crates/frees-core` — the engine (target-agnostic; **must never depend on wasm-bindgen**)
- `crates/frees-wasm` — thin wasm-bindgen boundary (JSON-string in/out)
- `crates/frees-cli` — headless solve/check for the parity harness
- `tools/golden-dumper` — Java program run against the frEES core jar to emit `fixtures/golden/`
- `fixtures/` — parity corpus + golden results; grow it per `fixtures/README.md`

Contract files (`ast.rs`, `token.rs`, `diag.rs`, `parser/mod.rs`, `units/quantity.rs`)
define fixed interfaces; change them deliberately, not incidentally. Unsupported
DSL blocks must fail with an explicit error — never silently skip. Diagnostics
are source-mapped and quote the user's text (parent-engine rule).

## What frees is, and what "browser-native" means here

**frees** (in `../frEES`, a separate git repository) is a web-based declarative equation-solving and acausal system-modeling environment: Java/Spring Boot backend + React 19/TypeScript frontend, deployed as Docker containers with RabbitMQ task dispatch and a Redis job store.

Today a solve is a network round-trip: editor text → `POST /api/solve` → RabbitMQ → compute worker → Redis → frontend polls `GET /api/jobs/{jobId}`. The goal of this project is to collapse that entire loop into the browser tab: the parser, unit checker, blocker, and solvers run as WASM, and the "server" disappears.

**`../frEES` is a read-only reference.** Read it freely to understand the engine; do not modify it from this repo.

## The reference implementation — read these first

| Path | Why it matters |
|---|---|
| `../frEES/CLAUDE.md` | Authoritative engine semantics: solver principles, the component/connect layer, the four domains, the deployment foot-guns. Read this in full before designing anything. |
| `../frEES/README.md` | Full system design + Agile plan. |
| `../frEES/docs/roadmap.md` | What's shipped vs deferred. Note: the WASM port is listed under **"Decided against (not deferred — closed)"** for frEES itself. |
| `../frEES/docs/critique-evaluation-2026-07.md` | The rationale for that decision — and the reason a port is *feasible*: "plausible long-term precisely because `core` is Spring-free by design". |
| `../frEES/backend/core/src/main/antlr/Frees.g4` | The 632-line grammar that defines the frees DSL. The port's source of truth for syntax. |

Because `../frEES` closed this port on its own roadmap, this repo is an independent exploration. Nothing here should assume changes will be accepted upstream.

## Port map: what carries over, what evaporates

The backend is a Gradle multi-module build, and the split was designed for exactly this kind of reuse:

- **`backend/core` (~134 files, ~38k LOC Java) is the port target.** Pure computation, zero Spring/AMQP/Redis: AST + parser, unit checker, Newton solver + Tarjan blocking, component expansion, ODE/DAE integrators, CAS, fluid/gas property models. Packages: `ast/`, `parser/`, `core/` (+`core/ode`, `core/dae`), `units/`, `props/`, `cas/`, `api/`, `measurement/`.
- **`backend/web` (~28 files, ~6.8k LOC) mostly evaporates.** RabbitMQ dispatch, Redis job store, `RequestGuardFilter` rate limiting, CORS, nginx proxying, client-IP trust — all of it exists because compute is remote. In-browser, none of it has a job. What survives is its **contract**: the REST endpoints define the surface the WASM module must expose as function calls.
- **The frontend is reusable as-is.** React 19 + TypeScript + Mantine (dark theme), CodeMirror editor, Plotly, Excalidraw whiteboard. The swap point is narrow: only **two** files issue HTTP calls — `frontend/src/api.ts` and `frontend/src/analyzer/measurementApi.ts`. That is the seam where a WASM binding replaces `fetch`.

### REST surface a WASM engine must cover

`/api/solve`, `/api/solve/table`, `/api/solve/montecarlo`, `/api/check`, `/api/repl/evaluate`, `/api/repl/clear`, `/api/optimize`, `/api/optimize/multi`, `/api/curve-fit`, `/api/plot/propplot`, `/api/plot/psychart`, `/api/control/plant`, `/api/control/pidtune`, `/api/reference`, `/api/fluids`, `/api/measurements/*`, `/api/jobs/{id}` (+ `/stream` SSE), `/api/health`.

The async job shape (`202 Accepted` + `jobId` + poll) exists only because of the broker. In-browser it becomes a Web Worker message round-trip — **keep the async shape at the `api.ts` boundary** so the UI doesn't need rewriting.

## Dependency substitution — the actual hard problem

Every `core` dependency needs a Rust/WASM answer. Ranked by risk:

| Java dependency | Used for | Port consideration |
|---|---|---|
| **Symja** (`matheclipse-core`) | CAS: Factor/Apart/Laplace/InverseLaplace/Diff/Integrate, symbolic `ss↔tf` | **Highest risk.** Large pure-Java CAS with no equivalent Rust crate. Options: keep CAS server-side (hybrid), reimplement the narrow subset actually used (`cas/` is 11 files), or evaluate `symbolica`/`egg`. Decide this early — it may define the project's scope. |
| **CoolProp** (via JNA) | Real-fluid + humid-air properties | **Lowest risk, highest value.** The binding surface is only four C functions: `PropsSI`, `Props1SI`, `HAPropsSI`, `get_global_param_string` (`props/CoolProp.java`, 215 lines). CoolProp is C++ and has an established Emscripten build — compile to WASM and keep the same four-call façade, including the existing LRU caches. |
| **SUNDIALS IDA + KLU** (via JNA) | Transient DAE solve, sparse steady | C library; Emscripten-compilable, or replace with a Rust integrator. Note the parent repo's SUNDIALS-v6-vs-v7 ABI trap (`SUNContext`, MPI linkage) — a WASM build sidesteps the distro-version problem entirely. Bindings are small: `SundialsIda.java` (207) + `SparseSteadyKlu.java` (151). |
| **ANTLR 4** | `Frees.g4` → parser/visitor | Rewrite as a Rust parser (`pest`, `chumsky`, `lalrpop`, or hand-written recursive descent). The grammar file is the spec; `parser/AstBuilder.java` shows the intended AST shape. |
| **Apache Commons Math** | Newton–Raphson, Jacobians, SVD, Brent, eigen-decomposition, LQR Riccati, LM curve fitting | `nalgebra` / `faer` for linear algebra, `argmin` for optimization. Watch numerical parity — the solver's step-halving behavior is load-bearing. |
| **JGraphT** | Tarjan SCC blocking | `petgraph` has Tarjan SCC directly. Straightforward. |
| **mdf4j** | ASAM MDF4 measurement files | Optional for a first cut; the parent already isolates it behind a `MeasurementParser` interface and has an out-of-process `mdf-sidecar`. |
| **Jackson** | JSON payloads | `serde` / `serde_json`. The DTOs in `api/SolveDtos.java` define the wire format the frontend already parses. |
| **FOP transcoder** | SVG rendering | Browser renders SVG natively; likely deletable. |

## Engine invariants to preserve (from `../frEES/CLAUDE.md`)

A port that breaks these isn't frees:

- It is an **equation solver**, not a sequential language — equations are order-independent.
- Variable names are **case-insensitive**.
- **All calculations run in SI**; unit-annotated inputs convert at parse time, computed variables get dimensionally derived SI units, and **unit warnings never block solving**.
- Types come from naming convention: `$` → string, `#` → constant, `[]` → array, `_r`/`_i` → complex components.
- Solve = Tarjan SCC blocking → per-block Newton with step-halving.
- The **component layer is a parser/expander, not a second solver** — `COMPONENT`/`connect` expands to scalar equations that flow through the same Newton/Tarjan path. ~295 components ship; connector-domain separation is a hard parse error by design, not a warning.
- Diagnostics are source-mapped to component names, never to mangled scalars.

## Browser-specific design constraints (new, no upstream precedent)

- **Solving must not block the UI thread** — the compute engine belongs in a Web Worker; `api.ts` becomes a `postMessage` shim wearing the current async interface.
- **Threads need cross-origin isolation.** `wasm-bindgen-rayon`/SharedArrayBuffer require COOP/COEP headers. Decide early whether to require them, because it constrains hosting and embedding.
- **Payload size is a product constraint.** CoolProp's fluid data plus the component library is not small; plan for lazy-loaded WASM chunks and measure. The parent has no bundle-size budget to inherit — set one.
- **Memory is bounded** (wasm32 = 4 GB ceiling, realistically far less). The Java engine assumed a JVM with `maxHeapSize = 2g` for tests.
- **`f64` determinism differs** from the JVM. Where numerical parity matters, port the parent's tests as the oracle (`../frEES/backend/core/src/test/`).

## Working with the reference repo

```bash
# Run the reference stack to compare behavior (Docker; never start host processes for it)
cd ../frEES && ./frees.sh start        # frontend :5173, backend API :8080/api
cd ../frEES && ./frees.sh logs         # follow
cd ../frEES && ./frees.sh stop

# Reference engine tests — the behavioral oracle for the port
cd ../frEES/backend && ./gradlew :core:test
cd ../frEES/backend && ./gradlew :core:test --tests "com.frees.backend.core.EquationSystemSolverTest"
```

Module-qualify single-test runs (`:core:` / `:web:`); an unmatched module fails the whole build.

`../frEES` has CodeGraph initialized (`.codegraph/`). Per the global instructions, explore it via an Explore agent using `codegraph_explore` rather than calling that tool in the main session.

## Toolchain state (as of this file's creation)

Nothing WASM-related is installed yet. `rustc 1.75.0` exists at `/usr/bin/rustc` (a distro/source-tarball build), but there is **no `cargo`, no `rustup`, and no `wasm-pack`**, and no `wasm32-unknown-unknown` target. `node v20.20.2` / `npm` are available.

Setting up `rustup` (which also supplies a current `cargo` and the wasm target) is step zero of any implementation work. Do not write build instructions that assume a toolchain that isn't installed — install it, verify it, then document what actually ran.
