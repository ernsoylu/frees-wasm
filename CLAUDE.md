# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository status

**Phases 0–7 are implemented** — the Phase-3 wasm boundary is wired, Phase 4
(differentiator, matrix, complex, procedural, tables, integrals, kernels, latex,
solver retry ladder) is complete, Phase 5 ports `props/` in full and gives the
browser a **working real-fluid property backend**, Phase 6 ports the acausal
**component / `connect` layer** together with the 295-component standard library
as embedded `.frees` data, and Phase 7 wires the **transient path**: `DYNAMIC`
and `LINEARIZE` parse and reach the engine, ODE Tables publish on the wasm
boundary, and the parity replay compares them. A Rust workspace ports the frees
engine to WebAssembly. Solve, check, transient integration, real-fluid
properties, property diagrams, component networks, the component datasheet, the
language reference, the CAS REPL, the control-systems `CALL`s and the Data
Analyzer's `.mf4` reading all run in-browser with **zero `/api/` traffic**.
Current gate numbers live in
[`docs/status-phase12.md`](docs/status-phase12.md) — do not trust a count copied
into this paragraph.

**MDF4 is removed (decision [D6](docs/decisions/0006-remove-mdf4.md), after
Phase 12).** The `.mf4` reader, `mf4-rs` (and its `meval` → `nom 1.2.4`
future-incompat debt — the build's only such warning, now zero), the
boundary's opened-file registry, and the analyzer's remote-source path are
gone; **the Data Analyzer is CSV-only** and `measurement_calc` survives,
stateless, with inline inputs. The Phase 10 paragraph below is therefore
**historical**: its measurement surface shipped, was hardened, and was then
deliberately removed — read D6 for why, and ledger item 37. The binary
fixture it says must be committed no longer exists.

**The dead-end UI is clipped (decision
[D5](docs/decisions/0005-feature-clip.md), after Phase 12).** The Min/Max,
Curve Fit, PID Tuner, Monte Carlo and Parameter Estimation modals and the
PDF/EPS exports are removed — they only ever surfaced `NOT_IN_BROWSER_ENGINE`
stubs. The stubs, the pid helpers and Phase 8's `analysis/` module remain as
the wiring seam (ledger item 36); the Tables workbook stays (its GUI Solve is
also stubbed but the workbook is core UI — a wire-next candidate, not a clip).

**Phase 12 is implemented — the plan's last phase.** A hardening pass that
changed **zero engine lines**: the parity corpus grew **531 → 701** (a
212-candidate harvest of the Java test classes via
`tools/harvest-java-tests/harvest.py`, run against the live oracle **on
macOS** — `COOLPROP_LIBRARY` must point at the `/usr/local/lib` dylib, never
the vendored Linux `.so`, or fluid goldens silently record failures), with all
21 non-promoted fixtures classified in `fixtures/README.md` (6 witnesses of
already-ledgered divergences, 12 unported features incl. the newly-found
unwired `CALL eigenvalues`/`eigen` — ledger item 34 — and 3 oracle artifacts).
Property-based fuzzing landed (`tests/fuzz_properties.rs`, proptest, 7
properties over parser/units/solve/MDF4 — dev-and-native-only, cfg-gated off
wasm32 because proptest's getrandom refuses that target), the first benchmark
suite (`benches/solve_bench.rs` + a JVM-oracle comparison: **~1× on the
transient**, integrator-bound as predicted, up to ~60× on scalar/property
documents), the first **named** bundle breakdown (twiggy over the pre-bindgen
artifact; the property tables and CAS/control confirmed as the two real
levers), and the worker-death/respawn path finally has tests
(`web/src/wasm/engineClient.test.ts`). Read
[`docs/status-phase12.md`](docs/status-phase12.md)'s "did not deliver" list
before extending: the harvester's representable-document boundary and the
frozen SUNDIALS oracle are the sharpest edges.

**Phase 11 is implemented and proven.** The browser-native product layer:
the app is an **installable PWA** whose Workbox service worker precaches the
entire built app (334 entries / ~30 MB incl. the wasm engine — offline reload,
offline solve and the offline project library are proven by a Playwright run
against a dumb static server in `docs/status-phase11.md`), an **IndexedDB
project library** (`web/src/projectStore.ts`, decision
[D4](docs/decisions/0004-project-storage.md)) gives Save-to-browser /
Browser-Projects with name-keyed file semantics, and the workspace autosave is
**dual-written** — localStorage stays the synchronous boot cache, IndexedDB is
the durable mirror that survives quota and is *offered* (never forced) as a
restore when it is strictly newer at boot. The web deploy is **static-only**:
the nginx `/api` proxy blocks, rate limiter and real-ip machinery are deleted,
and the Dockerfile fails fast when the generated wasm pkg is missing (it also
now actually copies `public/`, which the old image never did). **No Rust
changed** — the wasm bundle is untouched at 2944 KiB and all 531 fixtures
still match. Zero new runtime dependencies; `vite-plugin-pwa` and
`fake-indexeddb` are dev-only. Share links and `.frees` file save/open
predate the phase (vendored) and are documented as such, honestly, in the
status doc — read its "did not deliver" list before building on this layer;
item 1 is that the optional remote-fallback adapter stays unwired by choice.

**Phase 10 is implemented and wired.** `crates/frees-core/src/measurement/`
(MDF4 reading over `mf4-rs`, sampled series, envelope decimation, raster
construction, calculated signals) and `crates/frees-wasm/src/measurement.rs`
replace `/api/measurements/*`, and `web/src/analyzer/measurementApi.ts` now
calls the worker instead of `fetch`. **A `.mf4` opened in frees never leaves the
machine** — that is the point of the phase, and it is why the Java's
`Mf4Parser` → `FallbackMeasurementParser` → Python `mdf-sidecar` ladder collapses
to one rung. The cost of collapsing it is real and is the first entry in
[`docs/status-phase10.md`](docs/status-phase10.md)'s gaps list: **compressed
(`##DZ`) recordings — deflate, ZSTD and LZ4 — are refused, as are VLSD string
storage and multi-group files.** An OEM recording will probably not open.
One new dependency, `mf4-rs` 3.6 (MIT), which brings the repo's only
future-incompat warning (`meval` → `nom 1.2.4`); see that document for the exit.
The bundle is **2944 KiB against the 3072 KiB budget — green, with 128 KiB of
headroom**, and `fixtures/measurement/a_small_uncompressed.mf4` must be committed
with the `.rs` files or the core test build fails.

**Phase 9 is implemented and wired.** The Symja-replacement CAS
(`crates/frees-core/src/cas/`: exact rational algebra over ℚ, a Zassenhaus
factoriser, partial fractions, a Laplace transform table, `CasEngine` and
`CasIdentity`) and the control-systems suite
(`crates/frees-core/src/control/`: transfer functions, state space, LQR/place/
LQE, PID tuning, time responses, and the 41-name `CALL` flattener) both reach a
document, the REPL and the browser. Four new dependencies —
`num-bigint`/`num-integer`/`num-rational`/`num-traits`, all MIT OR Apache-2.0.
**531/531 fixtures** now match the oracle. One thing to know before building on
it: **`Integrate` is a closed pattern table, not an integrator**
(`docs/status-phase9.md` lists the exact boundary). *Correction:*
`docs/status-phase9.md` records the bundle as 3336 KiB and "the one red gate" —
that number is **pre-commit and stale**. The `opt-level = "s"` lever it describes
as "measured, not taken" was taken in the same commit, and re-measured at HEAD it
is worth 535 KiB. See `docs/status-phase10.md`'s bundle section.

**Phase 8 is *mostly* not implemented.** `crates/frees-core/src/analysis/` exists
and is unit-tested, but only **one** of its modules is reachable: `uncertainty`
is now wired into `engine::solve_with` at the Java positions, so
`UncertaintyOf(X) = expr` is lifted out of the equation stream, the propagation
and its second solve pass run, and `VariableDto.uncertainty` +
`uncertaintyBreakdown` reach the wasm boundary. The optimizer, NSGA-II, curve
fitter, Monte Carlo, parameter fit, all-roots solver and parametric sweep driver
are still unreachable from a document and absent from the boundary. A robustness
pass over Phases 7–8 hardened both surfaces and is written up in
[`docs/status-phase78.md`](docs/status-phase78.md); read its gaps list before
planning further work.

> **The web test gate needs Node 22.** Under `node v20.x`, `npx vitest run`
> fails all 38 files in `jsdom`→`undici` (`webidl.util.markAsUncloneable is not
> a function`) before running a test. `web/.nvmrc` asks for 22; use it.

**The property coverage gap is narrowed (decision
[D7](docs/decisions/0007-auxiliary-property-grids.md), 2026-08-06).** The
long-running "real-fluid coverage the tables do not have" divergence (ledger
item 9) cost the catalog its flagship Systems example: `ev-thermal-management`
failed at `Block 3 (89 equations): no property table for fluid
'INCOMP::MEG[0.50]'`, and behind that lay **three more** missing capabilities —
`R1234yf`, air transport for `htc_extair`, and saturation-line transport for
`htc_evap`/`htc_cond`/`dp_2phase`, which was absent for *every* fluid including
the two already tabulated. All four are closed by one new artifact kind
(`FRAUX1`) plus an ordinary `table-gen` run. **The example solves** — 169 blocks,
231 Newton iterations, COP 3.75, with its open-circuit energy balance closing to
9e-13 W — and grades against the JDK oracle at `8.951e-4` worst variable,
`1.0e-6` median over 229 variables. Corpus 702 → **704**. It cost the bundle
budget a 273.1 KiB breach, which is **since closed** — see the size pass below.

**The size pass is done (2026-08-06), and it changed no engine behaviour.** The
wasm is **3031.0 KiB raw / 1589.9 KiB gzipped, back under the 3072 KiB budget
with 41 KiB spare**, and the built web app is **20.25 MB → 14.7 MB of `dist`**.
All 704 fixtures still match, `cargo clippy` (native + wasm32) and `cargo fmt`
are clean, and vitest is 40 files / 384 tests green. Four findings, each
measured:

* **D7's "compression is not a lever" was wrong** — correctly measured, wrongly
  concluded. Plain deflate reaches only 0.89 on the `f32` property grids, but
  transposing each artifact into byte planes first (the HDF5 shuffle filter)
  takes 1014 KB → 685 KB, because the near-constant exponent bytes of thousands
  of consecutive samples then sit together. `crates/frees-core/build.rs` packs,
  `props/tables.rs` unpacks once at install time; it is a pure byte permutation
  plus a lossless codec, so the decoders still see the generator's own bytes and
  the artifacts under `src/props/data/` and `fixtures/` are untouched. One new
  dependency (`miniz_oxide`, MIT, inflate only, 11 KiB of code section).
* **Univer's ~80 hyphenation dictionaries were still being emitted** — 4.4 MB of
  `dist`. They had been excluded from the precache, which stopped the download
  but not the build output. They are stubbed at resolve time now. This also
  fixed a latent offline bug: Univer's `DocumentSkeleton` constructor eagerly
  loads the `en-gb` dictionary, so every spreadsheet render fetched a 102 KB
  chunk the service worker had been told to ignore. All ~80 imports redirect to
  one shared stub chunk, which is why `dist` is **95 files where it was 171**.
  The plugin needs `enforce: 'pre'` — without it Rollup resolves the real
  dictionary first and the 4.4 MB come back *with a green build*; the precache
  `globIgnores` list is kept as the backstop that catches exactly that.
* **The App chunk statically imported 1068 KB of reference documentation on
  every cold start**, to reach one 9 KB default-example string that Rollup's
  auto-placement had put in the `docs-data` chunk. Split. Opening the Examples
  modal likewise pulled the whole reference catalog; it now pulls 51 KB.
* **The nginx image served the entire bundle uncompressed** — no `gzip`
  directive at all, 19.3 MB on the wire where 6.2 MB would do. Only the Docker
  path was affected (Vercel compresses at its edge), which is why it went
  unnoticed. `gzip_static` plus precompression in the build stage.

One more, latency rather than bytes: KaTeX's stylesheet was imported at the
entry, and because `manualChunks` routes anything matching `/katex/` into the
`katex` chunk, that made its 254 KB of JS a static import of the entry — and
through the chunk graph, of every chunk in the app. KaTeX renders only inside
`<Latex>`, which only the lazy Help page mounts (the `$$…$$` blocks in
`symbolic_cas.md`, `language_fundamentals.md` and `tutorials.md`). The import
now lives in `Latex.tsx`.

| Document | Contents |
|---|---|
| [`docs/decisions/0007-auxiliary-property-grids.md`](docs/decisions/0007-auxiliary-property-grids.md) | **D7 — read before touching the property backend or the bundle.** The `FRAUX1` grids: what the three surfaces are, why saturated transport is 1-D (and therefore cheap), why the incompressible grid is *exact* in pressure, why its concentration axis lands on exact nodes, the measured error per grid, the one operating-point amplification that sets `ev-thermal-management`'s tolerance, and the budget breach with its four measured options |
| [`docs/status-phase12.md`](docs/status-phase12.md) | **Read first.** Phase 12 — parity at scale, performance, hardening: the corpus at **701** with the 21-fixture triage, the macOS-oracle traps (CoolProp dylib, frozen SUNDIALS), the proptest fuzzing contract, the measured Rust-vs-JVM table with its honest ~1× transient anchor, the named twiggy bundle breakdown, the worker-respawn tests — and a ranked list of what Phase 12 did **not** deliver, starting with the harvester's representable-document boundary |
| [`docs/status-phase11.md`](docs/status-phase11.md) | Phase 11 — the browser-native product layer: the installable-PWA + full-precache offline story with its browser proof (offline reload, offline solve, offline project library, zero network), the IndexedDB project library and dual-written autosave (decision D4), the static-only deploy, what was **already there** (share links, `.frees` save/open — stated plainly rather than claimed), and a ranked list of what Phase 11 did **not** deliver, starting with the still-unwired remote-fallback adapter |
| [`docs/status-phase10.md`](docs/status-phase10.md) | Phase 10 — measured data in the tab: what shipped per area, the **fifteen** defects three adversarial sweeps found — listed individually so the count is checkable (seven end the session — five allocation aborts and two unbounded walks — two are silent wrong answers, one wedges the worker on an ordinary formula, and five are numeric-parity divergences against a live JDK oracle) — the browser proof against genuine asammdf bytes with **zero `/api/` requests**, the raw gate numbers, the measured `mf4-rs` bundle delta and the `nom 1.2.4` debt with its exit — and a ranked list of what Phase 10 did **not** deliver, starting with the `mdf-sidecar`'s three compressed formats, which now have no answer at all |
| [`docs/status-phase9.md`](docs/status-phase9.md) | Phase 9 — the from-scratch CAS and the control-systems suite, wired end to end: what shipped per area, the `O(n⁴)` CAS defect the adversarial sweep found and fixed (a 200-symbol `Expand` went from **256 s to 0.41 s**, and to **103 ms in the browser**), the twelve other attacks that found the bounds already in place, the browser proof, the raw gate numbers, the 31 promoted fixtures — and a ranked list of what Phase 9 did **not** deliver, starting with the exact boundary of `Integrate`. **Its bundle section is stale**: the 3336 KiB breach it reports was fixed by the `opt-level = "s"` change in its own commit; see `docs/status-phase10.md` |
| [`docs/status-phase78.md`](docs/status-phase78.md) | The Phase 7–8 robustness pass: the four defects an adversarial sweep of the transient and analysis surfaces found (two of them process *aborts*, one a silent wrong answer), the guards that close them, the browser proof, the raw gate numbers, and a ranked list of what these phases did **not** deliver — starting with "Phase 8's `analysis/` is not wired to anything" |
| [`docs/status-phase7.md`](docs/status-phase7.md) | What Phase 7 delivers per area, the raw gate numbers, which 29 fixtures were promoted, and its pending table — **annotated with what Phase 9 closed** — plus the one honest performance finding (`dyn_accessor_live`) |
| [`docs/status-phase6.md`](docs/status-phase6.md) | What Phase 6 delivers per area, the component-coverage numbers, what the component fuzz found, its gate numbers and fixture counts |
| [`docs/status-phase5.md`](docs/status-phase5.md) | Phase 5 per area, the measured table-vs-CoolProp error, and its pending-fixture table (annotated with what Phase 6 closed) |
| [`docs/status-phase4.md`](docs/status-phase4.md) | Phase 4 per area; its pending-fixture table is annotated with what Phase 5 closed |
| [`docs/status-phase1.md`](docs/status-phase1.md) | The maintained divergence ledger (items closed are struck through with a date; Phase 5 opened items 9–12, Phase 6 items 13–14, Phase 7 items 15–18, the Phase 7–8 robustness pass items 19–23, the Phase 9 robustness pass items 24–25, Phase 10 items 26–30), plus the Phase 0–3 inventory |
| [`PLAN.md`](PLAN.md) | The phased plan: architecture, decisions, parity strategy, 13 phases, risks |
| [`docs/dependency-map.md`](docs/dependency-map.md) | Every Java/native dependency → Rust replacement |
| [`docs/feature-inventory.md`](docs/feature-inventory.md) | All 134 `backend/core` files mapped to features and phases |
| [`docs/decisions/`](docs/decisions/) | D1 (precomputed `(P,h)` property tables), D2 (wasm32-unknown-unknown + wasm-pack), D3 (worker pool, no COOP/COEP), D4 (project storage), D5 (feature clip), D6 (remove MDF4), **D7 (`FRAUX1` auxiliary grids + the bundle-budget breach)** |
| [`fixtures/README.md`](fixtures/README.md) | The parity harness: corpus, golden fixtures, tolerance policy (incl. `fixtures/tolerances.json`), oracle-established ground truths |

## Build and test

```bash
export PATH="$HOME/.cargo/bin:$PATH"   # toolchain is rustup-installed; distro rustc is stale
cargo test --release --workspace       # all tests incl. the parity replay
                                       # (--release: the replay solves 707 documents)
cargo test -p frees-core --test parity # golden-corpus parity only
cargo test -p frees-core --test fuzz_properties        # property-based fuzzing
                                       # (PROPTEST_CASES=4096 for a longer soak)
cargo bench -p frees-core --bench solve_bench          # the Phase 12 benchmarks
cargo test -p frees-core --test props_robustness       # the property-surface fuzz
cargo test -p frees-core --test component_robustness   # the component-surface fuzz
cargo test -p frees-core --test cas_control_robustness # the CAS + control fuzz
cargo test -p frees-core --test dynamics_robustness    # the transient + analysis fuzz
                                       # (run this one in DEBUG too — the stack-overflow
                                       #  defect it found only reproduced unoptimised)
cargo test -p frees-core --test measurement_robustness # the calc-signal/raster fuzz
cargo test -p frees-core --test measurement_parity     # measurement vs the JDK oracle
cargo clippy --workspace --all-targets -- -D warnings   # CI gate
cargo clippy --workspace --target wasm32-unknown-unknown --all-targets -- -D warnings
cargo fmt --all --check                                 # CI gate
wasm-pack build crates/frees-wasm --release --target web --out-dir ../../web/src/wasm/pkg
tools/golden-dumper/run.sh             # regenerate golden fixtures from the Java oracle
tools/table-gen/run.sh                 # regenerate fixtures/proptables from native CoolProp
tools/aux-gen/run.sh                   # regenerate fixtures/auxtables (FRAUX1 grids, D7)
tools/aux-gen/run.sh --sweep           # its error-vs-resolution ladder; writes nothing
```

> **The oracle tools find `../frees` themselves.** `tools/frees-home.sh`
> resolves the reference repo as a sibling of this one (both the `frees` and
> `frEES` spellings), so `classpath.sh` and every `run.sh` work without
> `FREES_HOME`. They used to hard-code `/home/eren/dev/frEES`, which outlived
> the directory it named and made all three tools report "reference repo not
> found" on a checkout where the reference was sitting right next door.
> Building the oracle jar once is still required:
> `(cd ../frees/backend && ./gradlew :core:jar)`.

> **The oracle has CoolProp.** `tools/golden-dumper/run.sh` exports
> `COOLPROP_LIBRARY` itself (the 12.4 MB `libCoolProp.so` vendored in
> `../frEES/backend/core/native/`), so every real-fluid property call can be
> checked against ground truth instead of guessed. Verified:
> `h = Enthalpy(Water, T=300 [K], P=101325 [Pa])` → `112654.89965464505`.

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
- `tools/table-gen` — Java program run against native CoolProp to emit `fixtures/proptables/*.phtab`
- `tools/aux-gen` — sibling of the above, emitting `fixtures/auxtables/*.fraux` (D7): the incompressible glycols, air transport, and saturation-line transport
- `tools/shared/GenSupport.java` — the byte sink, SHA-256, JSON writer and CoolProp-version binding both generators need; compiled into each tool's own `build/` by its `run.sh` (there is no jar). A divergence here would mean two artifacts disagreeing about their own checksums, so it lives in one file
- `tools/frees-home.sh` — resolves the reference repo as a sibling; the one place that path is decided
- `fixtures/` — parity corpus + golden results; grow it per `fixtures/README.md`

The property backend is **linked into the binary**: `crates/frees-core/src/props/data/*.phtab`
are copies of `fixtures/proptables/*.phtab`, and `data/*.fraux` of
`fixtures/auxtables/*.fraux`, all packed by `build.rs`, `include_bytes!`d by
`props/tables.rs` and installed on the first `solve`/`check`. Regenerating them
means copying them across as well as into `fixtures/`.

There are **two** artifact kinds, and the difference is load-bearing:

* `FRPHTAB1` (`.phtab`, `tools/table-gen`) — a phase-split `(P,h)` table for a
  fluid **with a saturation dome**, storing `T`, `Dmass`, `Smass`. Water, R134a,
  R1234yf.
* `FRAUX1` (`.fraux`, `tools/aux-gen`, decision
  [D7](docs/decisions/0007-auxiliary-property-grids.md)) — a rectangular grid of
  named outputs over two axes, for the three surfaces that geometry cannot
  carry: the **incompressible glycols** (`INCOMP::MEG`/`MPG`, which have no dome
  at all), **air transport** at `(P,T)`, and **transport on the saturation
  line** (`viscosity`/`conductivity`/`Cpmass` at `Q=0`/`Q=1` only — which is the
  only place `htc_evap`/`htc_cond`/`dp_2phase` ever ask, and the reason that
  costs ~14 KB per fluid instead of ~256 KB).

Neither kind is embedded verbatim. `crates/frees-core/build.rs` transposes each
artifact into `f32` byte planes and deflates it; `props/tables.rs` inflates it
once, at install time, and hands the decoders the generator's own bytes. That
takes the nine files from 1014 KB on disk to **~678 KB of data section**, and is
what put the module back under budget at **3031.0 KiB against 3072 KiB** after
D7's 273.1 KiB breach. The shuffle is the load-bearing half: plain deflate only
reaches 0.89 on these grids (which is what D7 measured before concluding, wrongly,
that compression was not a lever), and byte planes take it to 0.68. **Regenerating
a table needs no extra step** — copy the new `.phtab`/`.fraux` into
`src/props/data/` as before and `build.rs` re-packs it; the checked-in artifacts
stay exactly as the generators wrote them. Read D7 before adding a fourth fluid:
the headroom is 41 KiB, which is less than one full `.phtab`.

The **component library is also linked in**, the same way and for the same
reason: `crates/frees-core/src/components/library-data/*.frees` is 122 KB of DSL
text embedded with `include_str!` and parsed by the ordinary front end. It is
**data — never hand-translate a component.** `engine.rs::expand_component_layer`
early-returns before touching any of it when a document declares no components.

Contract files (`ast.rs`, `token.rs`, `diag.rs`, `parser/mod.rs`, `units/quantity.rs`)
define fixed interfaces; change them deliberately, not incidentally. Unsupported
DSL blocks must fail with an explicit error — never silently skip
(`parser/toplevel.rs::unsupported_construct` is the seam; as of Phase 7 its list
is **empty** — every block form the grammar admits now has a home on
`Document`). Diagnostics are source-mapped and quote the user's text
(parent-engine rule).

**A solved `DYNAMIC` block puts nothing in `Solution::values`** — the trajectory
is a first-class `OdeTableResult` on `Solution::ode_tables`. Any check that
compares only `variables` therefore passes *vacuously* on a transient document;
`tests/parity.rs` compares `ode_tables` for exactly that reason, and
`fixtures/README.md` records the four perturbations that were used to prove the
comparison can fail.

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
