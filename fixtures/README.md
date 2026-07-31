# Parity fixtures

The correctness backbone of the port (`PLAN.md` §4). The Java engine has 1,237
JUnit tests; hand-translating 24,359 lines of test code would be the project's
biggest mistake. Instead both engines run the **same corpus** and are compared.

```
fixtures/corpus/*.frees   the documents (hand-authored + harvested)
fixtures/golden/*.json    what the Java engine produced for each
fixtures/proptables/      generated CoolProp property tables (not a parity artifact)
tools/golden-dumper/      the Java side that generates fixtures/golden
tools/table-gen/          the Java side that generates fixtures/proptables
```

`proptables/` is the odd one out: it holds **inputs to the Rust engine**, not
expected outputs. The `.phtab` files are the browser build's real-fluid property
backend (decision D1, `docs/decisions/0001-property-backend.md`); their format
and the measured tabulation error are documented in `tools/table-gen/README.md`.
They are regenerated with `tools/table-gen/run.sh`, never edited.

## Regenerating

```bash
tools/golden-dumper/run.sh                    # corpus -> golden
tools/golden-dumper/run.sh <corpus> <out>     # explicit paths
tools/table-gen/run.sh                        # -> fixtures/proptables
```

**`golden-dumper/run.sh` regenerates every `.frees` in the directory it is
pointed at.** Running it over `corpus-pending/corpus` while another change is
mid-flight will mint goldens for documents that are not ready for one. Point it
at an explicit output directory and diff before keeping the result.

`classpath.sh` locates the newest `core-*.jar` under
`../frEES/backend/core/build/libs/` and assembles the dependency classpath from
the Gradle cache. It needs no Gradle run and **never writes to `../frEES`**.

Two classpath hazards it handles, both found the hard way:

* The cache holds `antlr4-runtime` 4.7.2 **and** 4.13.2. If the old one wins,
  every parse dies in class-initialisation with a version mismatch. Pinned jars
  go first and 4.7.x is filtered out.
* Multiple SLF4J providers are present; Logback is excluded so the binding is
  unambiguous.

Override with `FREES_HOME`, `FREES_CORE_JAR`, or `GRADLE_CACHE`.

## Fixture format

```json
{
  "name": "canonical",
  "source": "x^2 + y^3 = 77\nx/y = 1.23456\n",
  "expect": {
    "variables":     { "x": 4.694012391660914, "y": 3.802174371161316 },
    "display_names": { "x": "x", "y": "y" },
    "block_count": 1,
    "error": null
  },
  "oracle": { "engine": "frEES backend/core (Java)", "generated_by": "tools/golden-dumper" }
}
```

A document that **fails** is as valuable as one that solves — the Rust engine
must fail the same way:

```json
"error": { "type": "SolverException", "message": "There are 2 equations and 1 variables. …" }
```

## Comparison policy

Compare by **relative tolerance, never bit-equality**. WASM `f64` arithmetic is
IEEE-754 deterministic, but transcendentals (`exp`, `ln`, `pow`, `sin`) are
unspecified and differ between the JVM and Rust's libm.

| Field | Rule |
|---|---|
| `variables` | relative tolerance `1e-9`; absolute `1e-12` near zero. A named fixture may declare a looser *relative* tolerance in `fixtures/tolerances.json` — see below |
| `display_names` | exact |
| `block_count` | exact — a different blocking is a real divergence |
| `error` | `type` exact; `message` **not** compared verbatim (see below) |
| `ode_tables` | one entry per `DYNAMIC` block, in declaration order. `name`, `method`, `columns`, `stopped` and each event's `name` are **exact**; `end_time`, every row cell and each event `time` take the same numeric tolerance as `variables` |

### `ode_tables` — why a transient fixture needs it

**A solved `DYNAMIC` block puts nothing in `variables`.** The trajectory is a
first-class ODE Table, so a transient document's `variables` map holds only its
analytic parameters — `dyn_plain_ode` has exactly `{k, Tinf}` in it. A fixture
compared on `variables` alone therefore passes **vacuously**: the integration
could be wrong, or missing entirely, and the gate would stay green.

A golden dumped *before* the dumper grew this section has no `ode_tables` key at
all, which the replay treats as "not a claim" rather than "no tables" — but if
the Rust engine produces a table where the golden makes no claim, the fixture
**fails** with a "re-dump this fixture" message. Absence is never allowed to hide
a transient.

The comparison was validated the way the harness itself was in Phase 1, by
perturbing a golden and watching the gate go red. All four perturbation classes
were observed and then reverted:

| Perturbation | Reported as |
|---|---|
| a row cell `47.59095803046333` → `47.6` | ``ode_tables[0] `cooling` rows row 1 col `temp` = … but Java got 47.6 (rel 1.9e-4, tolerance 1e-9)`` |
| the whole table deleted | `Java recorded 0 ODE table(s), Rust produced 1` |
| `method` `ode45` → `ode23` | ``ode_tables[0] `ascent` method = "ode45" but Java got "ode23"`` |
| a stop event's `time` shifted by 0.5 s | ``ode_tables[0] `ascent` events hit 0 (`apogee`) fired at 156.74… but Java got 157.24… (rel 3.2e-3)`` |

### `fixtures/tolerances.json` — the one relaxation, and its guards

Decision D1 resolves real-fluid properties from precomputed `(P,h)` tables whose
measured error against native CoolProp is `1e-7…1e-4` relative, while the
goldens hold values the native library produced. **No table-backed engine can
pass a `1e-9` gate on a document that calls a real-fluid property function** —
the gap is a property of the artifact, not a bug waiting to be found. D1 named
two honest ways out (a per-fixture tolerance, or shipping `coolprop.wasm` as the
accuracy path); this is the first.

It relaxes the *numeric* tolerance for a named fixture and nothing else —
`display_names`, `block_count` and the error classification are still exact for
every fixture in the corpus, and every fixture not named there is still held to
`1e-9`. Two guards stop it becoming a place to hide failures:

* a fixture named there but absent from `fixtures/golden/` **fails the gate**;
* a fixture named there that **passes at the default** fails too, so a tolerance
  that is no longer needed cannot sit in the file pretending it is.

Each entry must record the *measured* error and a `reason` naming the mechanism
that produces it. The parity test prints which fixtures used a declared
tolerance on every run.

Error *messages* are not compared literally. The Java engine emits long,
domain-specific guidance ("A common cause: an element chain with no constitutive
law for that quantity…"). Matching that prose is not the goal; matching the
*classification* is. Assert the error type and that the message names the same
offending variables.

## Ground truth this corpus already pinned down

Behaviours that would otherwise have been guesses, now settled by running the
oracle rather than reading the source:

| Fixture | Establishes |
|---|---|
| `arithmetic` | `2^3^2 = 512` — exponentiation is **right**-associative. `-2^2 = -4` — unary minus binds **looser** than `^`. `-` and `/` are left-associative. |
| `units_negative_celsius` | `-10 [C]` = **263.15 K**, not −283.15. The unary sign folds into the literal *before* unit conversion (`AstBuilder.bareUnitLiteral`). |
| `units_pressure` | `140 [kPa]` = `140000.0` — SI conversion happens at **parse** time. |
| `units_temperature` | `25 [C]` = 298.15, `32 [F]` = 273.15 — additive offsets, `F` factor 5/9. |
| `intrinsics` | `sin(0)=0`, `cos(0)=1` — trig takes **radians**. |
| `constants` | `pi# = 3.141592653589793`, `R# = 8.314462618`, `g# = 9.80665`. |
| `case_insensitive` | `Tin`/`TIN`/`tin` are one variable; the result key keeps the **first-seen** spelling, and `display_names` maps lowercase → that spelling. |
| `sequential` | Blocks solve in **dependency** order, not source order (3 blocks). |
| `overdetermined` / `underdetermined` | Both are `SolverException`, and the message names the specific redundant relation / free quantity. |
| `empty` | A comment-only document is `SolverException: "No equations to solve."`, not success-with-nothing. |

## `fixtures/dae-oracle.json` — the API-level oracle

The odd one out in the other direction: not a document replay, but a dump of the
Java **DAE API** driven directly.

```bash
tools/dae-probe/run.sh            # -> fixtures/dae-oracle.json
```

It exists because a `.frees` document has no way to reach the implicit-DAE path
until the `DYNAMIC` grammar lands (`method = ida` on a `DYNAMIC` block is what
selects it), so `tools/golden-dumper` cannot produce ground truth for it. The
probe instead calls `IdaDaeSolver`, `DaeJacobian` and `SparseSteadyKlu` on
analytic problems — Newton cooling, a semi-explicit index-1 pair, Robertson
(with and without root finding), a 31-unknown C-R-C heat chain above the sparse
threshold, and a coupled algebraic loop — and records the trajectories,
consistent initial conditions, root times/directions, Jacobians, colourings and
sparse solves.

`crates/frees-core/src/dae/solver_tests.rs` and `dae/jacobian.rs` embed those
numbers as their `ORACLE_*` constants. **Re-run the probe rather than editing a
constant.**

> **It needs SUNDIALS ≥ 6.** `libsundials_ida.so.6` (the `SUNContext` API) must
> be loadable, plus `sunmatrixsparse`/`sunlinsolklu` for the KLU case. Without
> them the probe writes `"available": false` and exits 0 — the same graceful
> degradation `SundialsIda.isAvailable()` gives the Java engine — so a machine
> without SUNDIALS cannot silently mint an empty oracle over a real one.

## Growing the corpus

The 17 Phase-1 seeds (scalar equations, units, precedence, blocking, error
paths) have been joined by the harvested example documents, the hand-authored
Phase-4 cases that passed promotion, and the adversarial probe documents written
to hunt Java divergences (matrix naming and determinants, PROCEDURE/FUNCTION/
MODULE scoping, TABLE log-space and family interpolation, `Integral` accuracy
and its variable pin, solver bounds/root selection, units, operator precedence).
See **Pending corpus** below for the staging area and what is still blocked.
Extend it further from, in rough order of value:

1. `web/src/examples.ts` + `web/src/defaultExample.ts` — **harvested**; all 48
   are staged, 12 promoted
2. `../frEES/frontend/src/docs/*.md` (documented snippets)
3. the `*ExamplesTest.java` classes (`CycleExamplesTest`, `HvacExamplesTest`,
   `SystemDesignExamplesTest`) — already whole-document round-trips
4. `../frEES/backend/core/src/main/resources/components/*.frees` — all 295
   library components, once the component expander lands (Phase 6)

Add the document to `corpus/`, rerun `run.sh`, and commit both the source and
the generated golden file. **Review the generated fixture before committing** —
it encodes whatever the Java engine does, including any bug.

### The `components_*` group (Phase 6)

46 fixtures whose stem starts with `components_`, in three provenance classes:

| Prefix | Count | Where it came from |
|---|---|---|
| `components_family_<domain>` | 12 | **Hand-authored here**, one per domain family (`ac`, `electrical`, `fluid`, `heat`, `hydraulic`, `liquid`, `mechanical`, `moistair`, `pneumatic`, `powertrain`, `signal`, `twophase`). `control.frees` ships one component, `PIThermostat`, which rides the heat bond and is exercised from `components_family_heat`. |
| `components_<wave>_<name>` | 31 | **Harvested** from `../frEES/backend/core/src/test/resources/component-fixtures/`, renamed `<directory>_<stem>`. Only documents that reproduce the Java answer are here; the reference set's transient (`DYNAMIC`) documents are not, because the ODE engine is Phase 7/8. |
| `components_definition_*`, `components_user_defined_type` | 3 | Edge cases: a template that is never instantiated (empty document, and beside an unrelated scalar), and a user-declared `COMPONENT` shadowing nothing. |

Two things about this group differ from the rest of the corpus and are worth
knowing before extending it:

* **The golden's variable keys are display names.**
  `EquationSystemSolver.buildResult` keys `Result.variables()` with
  `displayNames.getOrDefault(name, name)`. For a scalar document that is the
  canonical name once lowercased, which is why folding the golden was enough for
  the pre-component corpus. For a component document it is not: the canonical
  name is `s2$p` and the display name is `s2.P`. `tests/parity.rs` therefore
  routes the *Rust* side through the same map before comparing — a no-op on
  every non-component fixture.
* **`crates/frees-core/tests/component_families.rs` measures coverage** over
  exactly this group, and pins two floors: how many of the 295 built-ins the
  corpus instantiates, and how many expand from a bare probe instantiation.
  Adding a document here can only raise those numbers.

Of the 92 documents in the reference component-fixture set, 51 are steady (the
rest need `DYNAMIC`); of those 51, **31 reproduce the Java answer** and are
promoted, and the other 20 fail on a property-backend limit, not the component
layer — `Air`/`CO2`/`Hydrogen`/`INCOMP::MEG` are not tabulated, `HAPropsSI` is
not implemented, and `Cpmass`/`viscosity` are not stored by the split `(P,h)`
table. Every one of those 20 names the missing capability in its error.

### The `av_*` / `pv_*` group (Phase 8 adversarial verification)

110 documents written to *hunt* divergences in the dynamics and analysis paths
rather than to record behaviour already believed correct. Each was authored,
run through **both** engines, and diffed on `variables`, `display_names`,
`block_count`, `error` classification and — for every transient — the whole
`ode_tables` section. Only documents that reproduced the oracle were promoted.

| Prefix | Count | What it pins |
|---|---|---|
| `av_int_` | 15 | Integrator agreement: Van der Pol through `ode45`/`ode23`/`ode23s` at 201 samples, a stiff scalar through all of `ode15s`/`ode23s`/`ode45`, a coupled stiff pair through the BDF path, fixed-step `ode1`/`ode4`, the dense-output interpolant sampled between steps, `maxstep` clamping, `rtol = 1e-12`, step rejection against a narrow Gaussian pulse, and the degenerate/reversed spans |
| `av_ev_` | 17 | Events: `rising` / `falling` / bare / explicit `any`; a guard exactly zero at `t0` (both `record` and `stop`); two guards crossing inside one step and the same two far apart — together those bracket `earliestEvent`'s **one hit per step** rule; two `stop` guards racing; a tangential touch that never changes sign; `set` restarting integration 27 times (thermostat) and 11 times (elastic bounce); `set` then `stop`; guards on an auxiliary and on `time`; a crossing exactly at `tf` |
| `av_acc_` | 14 | The twenty accessors read off a solved table: all ten on one column, on `time` itself, `ODEValue`'s clamp outside the span, `TimeAt` on a row / never reached / first of two crossings, an auxiliary column, two blocks with distinct and with identical column names, a spurious second argument, a missing required one, the null-context defaults, and the `augmentAccessorDependencies` rule that an unknown living only inside the block never becomes an analytic variable |
| `av_live_` | 5 | The live second-solve pass: `FinalValue` / `MaxValue` / `TimeAt` / `ODEValue` targets solved for an ODE input, and a 2×2 block whose every residual costs an integration |
| `av_stor_` | 5 | Storage → `DYNAMIC`: one and two `ThermalMass` lumps integrating, an electrical `Capacitor` (the routing is domain-agnostic), an event on a mangled port member, and the **steady limit** — the same network with no `DYNAMIC` block recovers the operating point the transient converges to (446.667 / 406.667 K) |
| `av_mol_` | 1 | Method of lines: a `FOR` loop in the block body over an array state through `ode23s` |
| `av_lim_` | 15 | Header and failure edges: `points` = 0 / 1 / 2 / negative, `step` past the span, `rtol = 0`, `atol = 0`, an unknown method, an unknown option, finite-time blow-up, a NaN rhs, the `span/100` default `maxstep`, a stop event's row grid, a unit on the span, an expression initial condition |
| `av_unc_` | 22 | `UncertaintyOf(X) = expr`: the square law, root-sum-square of two sources, a three-deep chain, one source feeding two dependents, propagation through an implicit 2×2 block, through the component layer, and through an **ODE accessor**; an expression-valued spec; zero / negative / 1e-12 sigmas; a query that reads its own dependent; a string-literal target; redeclaration; and the two shapes that are *not* declarations (`expr = UncertaintyOf(X)`, and a two-argument call) |
| `av_lin_` | 6 | `LINEARIZE`: default matrix names, 2 in / 2 out / 2 states, the 1- and 2-subscript spellings of a single-column `B`, the identity `C` row, an unknown block, an unknown output |
| `pv_` | 10 | `PARAMETRIC` declarations: a basic sweep, ragged columns, `Log` endpoints, and the seven range rejections |

Two things this group establishes that no earlier fixture did:

* **The uncertainty pass reaches the engine.** Before this phase
  `crates/frees-core/src/analysis/uncertainty.rs` was complete and unit-tested
  but had **zero call sites** — `UncertaintyOf(X) = 0.1` stayed in the equation
  list and every uncertainty document failed as overspecified. The `av_unc_`
  documents are the regression wall for the wiring.
* **A `DYNAMIC` document's cost is `maxstep`-driven, not span-driven.** With no
  `maxstep` the cap is `span/100`, which pins ~100 steps whatever the span is,
  and each implicit step's Newton residual re-blocks the algebraic system. Van
  der Pol through `ode15s` at 0..20 / 201 points costs **28.6 s here and 32 s in
  the Java oracle** — comparable, so it is a shared algorithmic cost and not a
  divergence, but it is why the promoted BDF probes are small and why
  `av_int_stiff_pair_ode15s` uses a linear 2-state system.

## Pending corpus

`corpus-pending/` is a staging area with the same shape as the promoted corpus
(`corpus-pending/corpus/*.frees` + `corpus-pending/golden/*.json`). Every
document there has a golden generated by the same oracle; none of them is
replayed by `crates/frees-core/tests/parity.rs`, so nothing here can turn the
gate red.

**The promotion rule:** run the document through the Rust engine
(`cargo run -qp frees-cli -- solve <file>`) and compare against its golden using
the tolerance table above — `variables` by relative tolerance, `display_names`
and `block_count` exactly, `error` by classification. If it agrees, move *both*
files into `corpus/` and `golden/`. If it diverges, leave it here. A pending
document that starts passing because someone fixed the engine is the point.

> **How to re-check the whole staging area at once.** Copy `tests/parity.rs`,
> point `golden_dir()` at `corpus-pending/golden`, and replace the panic at the
> end with a per-fixture `PROMOTE`/`HOLD` print. That reuses the gate's exact
> comparison logic instead of a hand-rolled approximation of it — which matters,
> because the `error` rule maps unrecognised Java exception types to "any Rust
> error" and will report the CoolProp eight as passing. Delete the copy
> afterwards. **Never relax `tests/parity.rs` to make something pass.**
>
> Last full re-check: **2026-07-30, Phase 5 property dispatch — 51 staged, 19
> promoted, 32 remain.** Wiring `props::*` into `eval.rs` (every `prop$…`
> synthetic, the seven `eos_*`, the five `AdiabaticFlameTemp*` spellings, the
> seven `mix_*`, `eq_molefraction`, the nine `htc_*`/`dp_*`) unblocked
> `adiabatic-flame-temp`, `chem_equilibrium`, `chem_errors`, `chem_flame_temp`,
> `chem_heating_value`, `chem_idealgas`, `chem_mixture`, `chem_molar_mass`,
> `chem_nasa7`, `cubic-eos-properties`, `eos-cubic-spot-probe`,
> `eos-cubic-sweep`, `gas-transport-mixture`, `karman-rocket`,
> `material-conduction`, `multi-objective-beam`, `solid-materials`,
> `solid_absent_property_rejected` and `solid_unknown_material_rejected`.
> Seven of those needed one further fix: a fluid/material/formula **token**
> (`MolarMass(C8H18)`, `k_(Steel)`, `Enthalpy(CO2, T=…)`) was being registered
> as a display name, because the Rust parser reaches it through `parse_expr`
> while the Java grammar has a dedicated token rule. `parse_call_atom` now
> un-registers it, first-wins-preserving (`Cursor::forget_display_name_if_new`).
>
> The previous re-check's nine green-scoring holds are down to one: the eight
> CoolProp goldens now fail honestly (`… needs a real-fluid property backend and
> none is installed`), because `prop$` dispatch reaches a real call instead of
> refusing the family. Only the rank-deficient `solver_singular_linear_cycle`
> is still a deliberate withhold.
>
> **Re-check 2026-07-31, Phase 6 component expansion — 29 staged, 0 promoted.**
> Wiring the expander into `engine.rs` moved the three component-bearing
> documents past expansion but not past their *other* blocker, so none of them
> is promotable and the pending set is unchanged in size:
>
> | Document | Now blocked on |
> |---|---|
> | `ev-battery-cooling-pid` | `DYNAMIC` — Phase 7/8 |
> | `pressure-cooker` | `DYNAMIC` — Phase 7/8 |
> | `ev-thermal-management` | **expands and blocks correctly** (89-equation block 3), then fails on `no property table for fluid 'INCOMP::MEG[0.50]'` — the D1 table limit, not the component layer |
> | `thermo-compliance` | `Z(R134a, P=1, T=1)` outside the generated table — D1 |
> | `state-tables-multifluid` | `STATE TABLE` block type still unparsed |
> | `hx-correlations-fluid` | `viscosity` is not a tabulated output — D1 |
> | `heisler-transient` | string variables (`geom$ = 'wall'`) |
> | `solver_singular_linear_cycle` | still the deliberate rank-deficient withhold |
> | the other 21 | `PLOT` (5), `DYNAMIC` (5 more), `PARAMETRIC` (3), `SYMBOLIC` (1), CAS/control `CALL`s (6), `MODULE` inside `FOR` (1) |
>
> Component coverage grew instead through a **new** group of 46 `components_*`
> fixtures (see "The `components_*` group" above), not by promoting from here.
>
> **Re-check 2026-07-31, Phase 7 transient wiring — 52 staged, 29 promoted,
> 361 → 390.** Making `DYNAMIC`/`LINEARIZE` parse and routing them through
> `engine.rs` promoted:
>
> * the 19 `dyn_*` probes, minus `dyn_accessor_live`;
> * `damped-oscillator-ode`, `newton-cooling-transient`, `transient-heat-rod`,
>   `sounding-rocket-trajectory`, `engine-cycle-wiebe` — five of the eight named
>   `DYNAMIC` documents;
> * `linearize-thermal-siso`, `linearize-thermal-2x2`;
> * `damped-oscillator`, `driving-cycle-energy`, `projectile-trajectory`,
>   `solver_singular_linear_cycle` (the deliberate withhold now agrees).
>
> **Five goldens had to be re-dumped first.** `damped-oscillator-ode`,
> `newton-cooling-transient`, `transient-heat-rod`,
> `sounding-rocket-trajectory` and `engine-cycle-wiebe` were dumped *before* the
> dumper grew its `ode_tables` section, so their trajectories were not in the
> fixture at all. Re-running `tools/golden-dumper/run.sh` over them is what makes
> their promotion mean anything — otherwise the replay would compare `variables`
> (which a transient document barely populates) and pass vacuously.
>
> The 23 that remain, by blocker — **none is blocked by the transient path**:
>
> | Blocker | Count | Fixtures |
> |---|---|---|
> | control-systems `CALL`s not ported | 11 | `control-analysis-report`, `controller-design-lqr-pid`, `cruise-control`, `digital-control-c2d`, `estimator-gramian-balreal`, `inverse-laplace-residue`, `multi-output-destructuring`, `nichols-chart`, `root-locus-analysis`, `routh-stability`, `step-impulse-response` |
> | property-backend limits (D1) | 6 | `adv_moistair_W_passthrough`, `adv_moistair_dryair_three_way`, `ev-battery-cooling-pid`, `ev-thermal-management`, `hx-correlations-fluid`, `thermo-compliance` |
> | `SYMBOLIC` / `MODULE` inside `FOR` | 2 | `partial-fractions`, `module_inside_for_loop` |
> | string variables in a numeric position | 1 | `heisler-transient` |
> | `method = ida` — the DAE path is assembled but not routed | 1 | `pressure-cooker` |
> | table-vs-CoolProp accuracy (worst 2.9e-6; needs a `tolerances.json` entry, deliberately not added) | 1 | `state-tables-multifluid` |
> | **cost, not correctness** — the live accessor converges to the oracle's `dk` to 4e-9 with a coarse `maxstep`, but does not finish in 7 min at the fixture's own `span/100` step cap. See `docs/status-phase7.md`. | 1 | `dyn_accessor_live` |

```bash
tools/golden-dumper/run.sh fixtures/corpus-pending/corpus fixtures/corpus-pending/golden
```

The staged set began as the 48 whole example documents from `web/src/examples.ts`
\+ `web/src/defaultExample.ts` (extracted verbatim, one file per `Example.id`)
plus 56 hand-authored Phase-4 documents mirroring the Java unit tests named in
each document's header comment. Adversarial verification since then added ~70
more probe documents written specifically to try to make the two engines
disagree; the ones that agreed were promoted directly.

> The harvest also turned up a **duplicate `Example.id` in `web/src/examples.ts`**:
> two different Thermodynamics entries both claim `rankine-cycle`, so the second
> is staged as `rankine-cycle-2.frees`. Anything keying examples by id (deep
> links, the command palette, `share.ts`) can only reach the first.

### What blocks the pending set

The `display_names` cluster (30 documents) and the unwired-kernel cluster
(`Integral`, `CALL Interp2`, `det$<n>` for n > 3) are **closed** — see the
engine-fix notes at the end of this section. What is left:

**1. Block types the wasm engine still refuses (0).** *Closed by Phase 7.* Every
block form the grammar admits now parses into `Document` —
`parser/toplevel.rs::unsupported_construct` returns `None` for every token. The
`PLOT` / `PARAMETRIC` / `STATE TABLE` three landed in Phase 8's parser work and
`DYNAMIC` / `LINEARIZE` in Phase 7's.

**2. Library calls not ported (12).** Control-systems `CALL`s (`lqr`, `lqe`,
`c2d`, `routh`, `residue`, `tf2ss`, `pole`, `nichols`, `rlocus`, `step`,
`ss2tf`) and string variables (`geom$`). The material database (`E_`, `k_`),
`MolarMass`, `eos_z` and `AdiabaticFlameTemp` closed in Phase 5.

**3. Pipeline-ordering deviation (1).** `module_inside_for_loop`: Java unrolls
`FOR` *during* flattening, so `CALL Twice(i : r[i])` inside a two-iteration loop
produces two module instances (`twice$1$…`, `twice$2$…`). This port flattens
CALLs in a pass that runs *before* unrolling, so it refuses the shape loudly
rather than grafting one instance across both iterations. Fixing it means moving
MODULE flattening past the unroller and re-basing the shared instance counter —
not a local change. The fixture records exactly what Java produces.

**4. Ill-posed by construction (1).** `solver_singular_linear_cycle` is
structurally square but rank-deficient (`x = y+1`, `y = z+1`, `z = x-2` reduce
to `x = z+2` twice), so its solution set is a *line*. Both engines return a
point on that line with a residual at machine zero — Java `(2, 1, 0)`, Rust
`(2+6.6e-14, 1+6.6e-14, 6.6e-14)`. That agrees inside the tolerance table today,
but promoting it would freeze an arbitrary point of a continuum into the gate.
Held deliberately, for the same reason as the CoolProp eight.

Separately, **8 documents have no usable golden**: the Java oracle refused them
with `IllegalStateException: The CoolProp native library is not available` on
this machine. Rust also refuses them, so a naive comparison scores them green —
that would bake a missing `.so` into the parity gate and turn it red the day
someone installs CoolProp. They are deliberately **not** promoted. Regenerate
their goldens with `COOLPROP_LIBRARY` set before trusting them.

### Engine fixes this staging area produced

Each of these was a genuine Rust-vs-Java divergence found by replaying a staged
or probe document through both engines, and each unblocked fixtures wholesale:

* **`display_names` was reconstructed, not recorded.** The port rebuilt the map
  with a lexer pass over the source and then filtered it to solved variables.
  That is wrong in both directions: unit spellings and TABLE column headers won
  the first-seen race against real variables (`cp = 1004 [J/kg-K]` bound
  `k -> "K"`, beating a later `k = 1.4`), while FUNCTION/MODULE body-locals,
  formals, integration variables, bare container names and the original casing
  of expanded elements were all missing. The map is now accumulated where Java
  accumulates it — `Cursor::record_display_name` at the two `AstBuilder` sites
  (`visitVarAtom`, `visitArrayAtom`), the CALL flattener's namespaced module
  variables, and an element rule replayed over the expanded system. 30 fixtures.
* **`det$<n>` was never dispatched.** `parser::expand` emits it for any `det(A)`
  larger than 3x3, and `linalg::eval_intrinsic` implements it (plus `qr$`,
  `chol$`, `expm$`, `svd$`) — but `eval::eval_synthetic` had no arm, so the whole
  module was unreachable from user text and every such document died with
  "not yet supported: det$4".
* **`CALL LUDecompose` and `CALL Interp2` were refused before their flatteners
  ran.** Java flattens PROCEDURE/MODULE calls and the matrix intrinsics in one
  pass; this port splits that in two, and stage 2 was refusing by name every
  intrinsic CALL — including the two whose flatteners live in stage 3. Both
  `expand::flatten_lu_decompose` and the new `expand::flatten_interp2` were
  dead code. `procedures::EXPANDED_CALL_TARGETS` now passes them through.
* **Ignored-output sinks leaked into the solution.** Omitting a trailing CALL
  output (`CALL LinFit(x, y : m, b)`) mints a hidden `~ignored~N` sink that the
  solver must determine but that Java never surfaces
  (`EquationSystemSolver.java:1888`). The port had `is_ignored_sink` but never
  called it on the result path, so `~ignored~0 = 1.0` showed up as a result row.
  `engine.rs` now filters it out of the values map, out of `check`'s variable
  list, and out of both reported counts (`surfacedVarCount` and
  `surfacedEqs = equations − (allVars − surfacedVars)`, so hiding a sink's
  variable also hides its equation). 2 fixtures.

### Authoring hazard: sink names carry a JVM-global counter

`EquationParser.IGNORED_SINK_SEQ` is a **static `AtomicLong`, never reset per
document**, so the `N` in `~ignored~N` depends on how many documents ran before
it *in the same JVM*. Running one document inside a batch produced
`~ignored~1`; running it alone produced `~ignored~0`, from the same engine and
the same source.

This is invisible for a **scalar** sink — Java hides it from `variables` *and*
`display_names`, so those fixtures are reproducible and are the ones frozen
(`call_linfit_omitted_r2`, `call_linfit_omitted_b_and_r2`). It is fatal for an
**array or matrix** sink: Java hides it from `variables` but **keeps** its
elements in `display_names` (`~ignored~1[1,1]` …), which `tests/parity.rs`
compares exactly. **Do not freeze a fixture whose golden mentions
`~ignored~N[`** — `[L] = LUDecompose(A)`, `CALL SVD(A : U, S)`,
`CALL QR(A : Q)`, `CALL FFT(re, im : out_re)` and the like. The Rust engine's
per-document counter is arguably better behaved and will never match the Java
batch value.

(Also: `~` is not user-writable syntax. `CALL QR(A : Q, ~)` is a Java
`ParseException` — sinks come only from *omitted trailing* outputs.)

### Still pending

| Document | Needs | Java solved it? |
|---|---|---|
| `adiabatic-flame-temp` | combustion kernel: `AdiabaticFlameTemp` | yes |
| `control-analysis-report` | PLOT blocks | yes |
| `controller-design-lqr-pid` | control-systems CALL: `lqr` | yes |
| `cruise-control` | PLOT blocks | yes |
| `cubic-eos-properties` | cubic-EOS kernel: `eos_z` | yes |
| `damped-oscillator` | PARAMETRIC blocks (Java instead reports underspecified) | no — `SolverException` |
| `damped-oscillator-ode` | DYNAMIC (ODE/DAE) blocks | yes |
| `digital-control-c2d` | control-systems CALL: `c2d` | yes |
| `driving-cycle-energy` | PARAMETRIC blocks (Java instead reports underspecified) | no — `SolverException` |
| `engine-cycle-wiebe` | DYNAMIC (ODE/DAE) blocks | yes |
| `estimator-gramian-balreal` | control-systems CALL: `lqe` | yes |
| `gas-transport-mixture` | `eval.rs` to dispatch `mix_viscosity` / `mix_conductivity` into `props::transport` (the physics is ported and unit-tested against these very numbers; only the registry arm is missing) | yes |
| `ev-battery-cooling-pid` | oracle unavailable — Java needs the CoolProp native lib | no — `IllegalStateException` |
| `ev-thermal-management` | oracle unavailable — Java needs the CoolProp native lib | no — `IllegalStateException` |
| `heisler-transient` | string variables (`name$ = 'text'`) | yes |
| `inverse-laplace-residue` | control-systems CALL: `residue` | yes |
| `karman-rocket` | property call: `MolarMass` | yes |
| `material-conduction` | material database: `k_()` | yes |
| `module_inside_for_loop` | MODULE flattening must run *after* FOR unrolling (see cluster 3) | yes |
| `multi-objective-beam` | material database: `E_()` | yes |
| `multi-output-destructuring` | control-systems CALL: `tf2ss` | yes |
| `newton-cooling-transient` | DYNAMIC (ODE/DAE) blocks | yes |
| `nichols-chart` | PLOT blocks | yes |
| `partial-fractions` | SYMBOLIC / CAS | yes |
| `pressure-cooker` | oracle unavailable — Java needs the CoolProp native lib | no — `IllegalStateException` |
| `projectile-trajectory` | PARAMETRIC blocks (Java instead reports underspecified) | no — `SolverException` |
| `rankine-cycle` | oracle unavailable — Java needs the CoolProp native lib | no — `IllegalStateException` |
| `rankine-cycle-2` | oracle unavailable — Java needs the CoolProp native lib | no — `IllegalStateException` |
| `refrigeration-vcr` | oracle unavailable — Java needs the CoolProp native lib | no — `IllegalStateException` |
| `root-locus-analysis` | PLOT blocks | yes |
| `routh-stability` | control-systems CALL: `routh` | yes |
| `solver_singular_linear_cycle` | nothing — held: rank-deficient, so the solution set is a line (see cluster 4) | yes |
| `sounding-rocket-trajectory` | DYNAMIC (ODE/DAE) blocks | yes |
| `state-tables-multifluid` | oracle unavailable — Java needs the CoolProp native lib | no — `IllegalStateException` |
| `step-impulse-response` | PLOT blocks | yes |
| `thermo-compliance` | oracle unavailable — Java needs the CoolProp native lib | no — `IllegalStateException` |
| `transient-heat-rod` | DYNAMIC (ODE/DAE) blocks | yes |

Two authoring hazards found the hard way, both worth repeating:

* **Brace comments do not nest.** A `{` inside a `{ … }` comment ends the
  comment at the first `}`, and the rest of the line is parsed as equations.
  `sum_{i,j=1..3}` in a header comment turned a working document into a syntax
  error and the golden dutifully recorded it. An unexpected `ParseException` in
  a generated golden usually means the *fixture* is wrong, not the engine.
* **`#` marks a built-in constant, not a user name.** `E# = 72000` is an attempt
  to redefine a built-in and makes the document overspecified; the activation
  energy has to be `Ea`.
