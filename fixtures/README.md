# Parity fixtures

The correctness backbone of the port (`PLAN.md` §4). The Java engine has 1,237
JUnit tests; hand-translating 24,359 lines of test code would be the project's
biggest mistake. Instead both engines run the **same corpus** and are compared.

```
fixtures/corpus/*.frees   the documents (hand-authored + harvested)
fixtures/golden/*.json    what the Java engine produced for each
tools/golden-dumper/      the Java side that generates fixtures/golden
```

## Regenerating

```bash
tools/golden-dumper/run.sh                    # corpus -> golden
tools/golden-dumper/run.sh <corpus> <out>     # explicit paths
```

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
| `variables` | relative tolerance `1e-9`; absolute `1e-12` near zero |
| `display_names` | exact |
| `block_count` | exact — a different blocking is a real divergence |
| `error` | `type` exact; `message` **not** compared verbatim (see below) |

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
> Last full re-check: **2026-07-30, Phase 4 close — 36 staged, 0 promotable.**
> The only nine that scored green were the eight CoolProp-poisoned goldens and
> the rank-deficient `solver_singular_linear_cycle`, all deliberately withheld
> (see below).

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

**1. Block types the wasm engine still refuses (14).** `PLOT` (5), `DYNAMIC`
(5), `PARAMETRIC` (3), `SYMBOLIC` (1). The `PARAMETRIC` three are *error*
fixtures: Java also refuses them (they are swept from the Tables tab, so solved
directly they are underspecified) — but Java raises `SolverException` where Rust
raises `ParseException`, so the classifications still disagree.

**2. Library calls not ported (12).** Control-systems `CALL`s (`lqr`, `lqe`,
`c2d`, `routh`, `residue`, `tf2ss`), the material database (`E_`, `k_`),
`MolarMass`, `eos_z`, `AdiabaticFlameTemp`, and string variables (`geom$`).

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
