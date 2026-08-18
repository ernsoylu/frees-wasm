# D9 — rustprop is the wasm build's only property backend, and the tables leave the bundle

**Status:** implemented
**Date:** 2026-08-17
**Implements** [D8](0008-coolprop-wasm.md). **Settles** the bundle question D8
left open. **Supersedes** [D1](0001-property-backend.md)'s *linked* tables for
the browser build only — the fetch seam D1 asked for is what survives.

## Decision

Four parts, and the fourth is what keeps the first three honest.

1. **The wasm bundle ships rustprop as its only in-bundle property backend.**
   `props::tables::install_builtin_once` — the function every public entry point
   calls — installs `RustpropBackend` when the `rustprop-backend` feature is on,
   and the pre-D9 `TableBackend` when it is off. `frees-wasm` turns it on.
2. **The linked `.phtab` / `.fraux` data leaves that bundle.** The
   `include_bytes!` calls in `props/tables.rs::packed` now sit behind a
   `linked-tables` Cargo feature, on by default and off in exactly one place:
   `frees-wasm` depends on `frees-core` with `default-features = false`.
   `build.rs` skips the deflate entirely when the feature is off, so a stray
   include is a build error rather than 684 KiB quietly returning.
3. **The decoders and the `install_from_bytes` seam stay compiled.** They are
   what is left of D1 in the browser: the offline / speed fallback a host can
   fetch into. Only the *bytes* are conditional — `unpack`,
   `SaturationSplitTable`, `AuxTable` and `install_from_bytes` compile in every
   configuration, and `BUILTIN_TABLES` / `BUILTIN_AUX` become empty slices
   rather than disappearing.
4. **Parity is pinned to one backend.** Wherever the accuracy path is compiled
   the gate grades *it*, through `fixtures/tolerances-rustprop.json`; the table
   configuration keeps `fixtures/tolerances.json` unchanged. The harness reads
   exactly one of the two, chosen by the same `cfg` that chooses the backend, so
   there is no configuration in which the gate grades a backend the product does
   not ship. Note *why* the native gate lands on rustprop without anything
   opting in: `cargo test --workspace` resolves `frees-core`'s features as the
   union over the members being built, and `frees-wasm` is one of them. CI's
   `native` and `parity` jobs therefore grade the same backend the browser gets.
   `cargo test -p frees-core` on its own does not unify, and grades the table
   path against `tolerances.json` — which is why that file had to stay correct
   rather than be edited.

## Context — the question D8 refused to guess at

D8 chose the accuracy path and named the constraint that would decide whether it
could ship:

> **Bundle budget is the gating constraint, and it is already tight.** The wasm
> is 3042.3 KiB against the 3072 KiB budget — **29.7 KiB of headroom**. […]
> Either the budget moves, or the `.phtab` / `.fraux` artefacts it replaces come
> out (~678 KiB of data section today), or the property backend lazy-loads as a
> separate chunk. This has to be settled as part of the integration, not
> discovered by it.

It is settled by the middle option, and the budget does not move. `ci.yml`'s own
comment block has asked twice for exactly this — "Move the 528 KB of linked
property tables onto the fetch seam that already exists" is debt item (1), open
since Phase 6 and unpaid through four phases.

## Measured — `wasm-pack build --release --target web`, the CI command

All four corners, so the two changes can be told apart:

| configuration | raw | gzipped | vs. budget |
|---|---:|---:|---:|
| pre-D9: linked tables *reachable*, no rustprop | 3042.3 KiB | 1597.7 KiB | 99.0 % |
| **shipped: rustprop, `linked-tables` off** | **2700.3 KiB** | **1109.5 KiB** | **87.9 %** |
| rustprop, `linked-tables` **on** | 2700.4 KiB | 1109.5 KiB | 87.9 % |
| neither — the engine with no property source | 2358.3 KiB | 912.8 KiB | 76.8 % |

* **the tables cost 683.9 KiB raw / 684.8 KiB gzipped.** Those two numbers being
  equal is the point D7 made and `build.rs` acts on: the artifacts are already
  deflated on the way in, so the browser's own gzip finds nothing left.
* **rustprop costs 342.0 KiB raw / 196.7 KiB gzipped** — well under the ≤545 KiB
  the integration was budgeted for, and it *compresses*, because it is code
  rather than `f32` grids.
* **net: −341.9 KiB raw, −488.2 KiB gzipped.** Headroom goes from 29.7 KiB to
  371.7 KiB. The gzipped column is what a browser downloads, and it is the
  bigger win: a strictly more accurate engine that is 31 % smaller on the wire.

### The row that changes what part 2 of this decision is *for*

Rows two and three differ by **one byte**. Feature-gating the `include_bytes!`
calls is not what removes the 684 KiB — **installing rustprop is**. The moment
`install_builtin_once` stops calling `install_builtin`, nothing in the wasm
crate's reachable graph touches `BUILTIN_TABLES`, and fat LTO drops the data
section on its own.

That does not make `linked-tables` redundant; it makes it a **guard rather than
a saving**, and the guard is the load-bearing half:

* Part 3 of this decision deliberately keeps `install_from_bytes` compiled as
  the fetched fallback. That function calls `builtin_tables()` and
  `builtin_aux()`. One `#[wasm_bindgen]` export wired to it — the obvious next
  step for anyone implementing the offline path — makes the whole 684 KiB
  reachable again, and the only symptom would be the budget step going red with
  no line of the diff explaining why.
* `build.rs` skips the deflate when the feature is off, so OUT_DIR is empty and
  an `include_bytes!` that escaped its `cfg` is a build failure, not a silent
  regression.

A saving that depends on the optimiser's reachability analysis is a saving that
one innocuous call re-spends. The feature makes it structural.

## What it clears

**Twelve of the 23 entries in `fixtures/tolerances.json` are dead.** They existed
because D1's tables sit 1e-7…1e-4 from CoolProp; rustprop *is* CoolProp 8.0.0, so
that error is gone. Measured on the switch-over run:

| fixture | table backend | rustprop |
|---|---:|---:|
| `sysdesign-ex11-liquid-cooling-loop` | 1.3e-4 | **0** |
| `sysdesign-ex17-ac-expansion-valve` | 7.9e-8 | 1.2e-16 |
| `components_wave2_flash_tank` | 5.3e-8 | 4.1e-16 |
| `components_wave2_ejector_oilsep` | 5.1e-8 | 2.7e-15 |
| `sysdesign-ex20-zeotropic-blend` | 2.9e-8 | 1.3e-15 |
| `components_family_fluid` | 1.4e-6 | 4.7e-14 |
| `rankine-cycle`, `rankine-cycle-2` | 6.4e-7 | 7.7e-14 |
| `components_family_liquid` | 1.2e-6 | 7.9e-14 |
| `components_wave8_hydro_turbine` | 1.6e-6 | 3.9e-13 |
| `components_bsweep_valve_characteristics` | 1.6e-6 | 4.2e-13 |
| `state-tables-multifluid` | 2.9e-6 | 4.4e-11 |

(eleven rows, twelve entries — `rankine-cycle` and `rankine-cycle-2` share one.)

`ev-thermal-management`, the loosest entry in the file at 2e-3, is not retired
but improves **178×** (8.95e-4 → 5.0e-6): its worst variables were a glycol
film coefficient off the `INCOMP::MEG` `FRAUX1` grid, amplified by the
Re = 2987 laminar/turbulent blend, and rustprop answers `INCOMP::MEG` exactly.

Eleven entries survive, and **not one of them is this port's error any more** —
the reasons in `fixtures/tolerances-rustprop.json` name the mechanism per
fixture. Ten are the *golden* side: `props/PropertyFunctions.java` answers output
`T`/`Dmass`/`Smass` at the input pair `(P, Hmass)` from its own run-time
256/96/48 table, gated at 1e-4, so those goldens are not CoolProp values and the
gap is the Java's interpolation error. `components_wave2_liquid_tms` shows it
cleanly: the document imposes 400 K at a source, converts to `h`, and inverts —
this port returns 399.999_999_91, the Java 400.000_678_12. The eleventh
(`refrigeration-vcr`, 3e-9) is cancellation in a COP.

> **Amended by Wave-3 F5** (below). Two claims in the paragraph above did not
> survive re-measurement. There are **ten** survivors, not eleven — the
> `solver_floor` entry died at Wave-2 integration. And `refrigeration-vcr` is
> **not** cancellation of otherwise bit-exact goldens: its leaf
> `Enthalpy(R134a, P, s)` is itself 1.6e-10 from the wheel, because upstream's
> own flash stopped 4.06e-9 short in pressure. The headline claim — *not one of
> them is this port's error* — held, and F5 proved it per fixture against the
> CoolProp 8.0.0 wheel rather than by inference.

D8 predicted the trap and it was real: the file's own rule is that an entry
passing at the default *fails*, so the dead entries had to go in the same change.
They could not simply be deleted, because the table configuration is still
supported and still needs them — hence two files, one per backend.

## Finding 1: iterative flashes have a noise floor

One fixture, `sysdesign-ex16-moving-boundary-evaporator`, stopped converging.
Its cause is general enough to belong in this record rather than in a comment.

A `(P,h)` table answers `Temperature(fluid, P, h)` from a bilinear surface, so the
residual `T_out − Temperature(P, out.h)` is smooth in `h` and Newton drives it to
the engine's 1e-12 default. rustprop answers the same call with an **iterative
flash**, converged to ~8e-10 relative in `h`. Measured on R134a at 350 kPa near
h = 423 193 J/kg: stepping `h` by 1e-3 J/kg moves `T` by 1.129_662_4e-6 K nine
times out of ten and by 7.531_083e-7 K the tenth. The step is a staircase, not a
slope, and no line search can descend a staircase. The engine's report — "no
full, halved or damped step reduces the residual" — is the truth.

This bites only where the *inverted* variable is unknown **in the same Newton
block**; ex16 is the one document in 707 that does that. The response is a
declared, measured, per-fixture stop-criterion relaxation
(`solver_floor` in the rustprop tolerance file: 1e-10 against a residual
bisected to between 5e-11 and 6e-11), carrying the same two honesty guards as a
numeric tolerance — an entry whose fixture converges at the default fails, and
an entry with no fixture fails. The **values** are still graded, at 1e-8. What
moved is only the point at which the solver stops chasing arithmetic noise.

The engine's global default was deliberately **not** touched: it is the Java
oracle's `SolverSettings.DEFAULTS` value, and loosening it for everybody to
rescue one document would trade a real parity property for a convenience.

> **RESOLVED 2026-08-18 — the noise floor was rustprop's, and rustprop fixed
> it.** This finding was written against a rustprop whose single-phase `(P,X)`
> flash still used a documented 30-bit bisection **stand-in** for upstream's
> solver; the ~8e-10 staircase measured above is that stand-in's granularity,
> not a property of iterative flashes in general. Wave-2 R8 replaced it with
> upstream's own Boost TOMS748 plus a warm-density carry across probes, and
> rustprop's own witness is that the median displacement from the CoolProp
> wheel over 1 433 `(P, caloric)` goldens moved 1.77e-10 → 2.04e-16. At Wave-2
> integration ex16's Newton block reaches the 1e-12 engine default unaided and
> the fixture grades at 4.207_713_303_802_4e-12 — under the 1e-9 default. Both
> of its entries were therefore deleted and the `solver_floor` section is now
> empty; the mechanism it guarded is real, so the section and its harness
> support stay, with no instance. The paragraph above is left as written
> because the reasoning was correct on the evidence available then, and
> because the honesty guards it describes are precisely what made the fix
> announce itself: the parity run failed with "delete the entry rather than
> leaving a dead relaxation in the file".

## Finding 2: the adapter has to enforce `RealFluid`'s contract, not just forward

`props_robustness`'s exhaustive key sweep — every output key × every input pair ×
a hostile value set — found two things a 1:1 forward cannot survive. Both are
rustprop being *correct*, and neither is usable through this seam.

**Non-finite out.** `PropsSI("Hmass", "T", 0, "Smass", 101325, "Water")` returns
`Ok(NaN)`: upstream's behaviour at that degenerate state, faithfully ported. But
`RealFluid`'s doc says an implementation "must decline (`Err`) rather than
extrapolate. A backend that answers outside its valid range is worse than no
backend, because a wrong answer reaches the user as a solved variable" — and a
`NaN` is the worst case, entering the Newton residual as data and converging
nothing while looking like progress.

**Non-finite in — and this one is not a nicety.**
`PropsSI("Dmass", "Smass", NaN, "Hmass", 101325, "Water")` **panics**, on
`assert!(fa * fb <= 0.0)` in rustprop's Chebyshev root finder: a bracketing
assertion is precisely what a `NaN` defeats. The shipped wasm is
`panic = "abort"`, so that is not a recoverable error — it kills the engine
worker and `engineClient` has to respawn it. And the inputs to a property call
are *document* values, which a diverging solve or a `props_si_or_nan` diagram
sweep produces as `NaN` routinely. A user document could have taken the engine
down.

`RustpropBackend` now guards **both** sides on all three calls: non-finite inputs
are refused before rustprop sees them, non-finite answers are refused before the
engine sees them. Both are pinned by tests, and the output test asserts
upstream's `NaN` *first*, so a future rustprop that throws there is a signal
rather than a silent pass.

The general shape is worth carrying into the rest of the integration: **the
adapter is not a pass-through.** Faithfulness to CoolProp is rustprop's contract;
survivability of hostile input from a Newton solver is this crate's, and they are
not the same contract. F1 wrote the forward; the sweep is what found the gap
between them.

## Consequences and open risks

* **GitHub CI cannot build the wasm or web jobs until rustprop is published.**
  `frees-core` reaches rustprop through a `path` dependency on a sibling
  checkout, which F1 made *optional* so the runner could still build. D9 makes
  `frees-wasm` require it, so the `wasm`, `web` and (through feature
  unification) `native`/`parity` jobs now need `../rustprop` present. This is
  the one thing in this record that is not self-contained: rustprop's crates.io
  names are claimed but unpublished, pending its owner's registry token. Until
  then the gate is a local gate. Nothing here needs to change when it lands —
  only the two dependency lines.
* **The fetch fallback is narrower than the seam suggests, and deliberately not
  widened here.** `install_from_bytes` takes one `FRPHTAB1` artifact and rebuilds
  the backend from the linked floor plus that artifact. With the floor now empty
  in the wasm build, (i) a second call replaces the first fluid instead of adding
  to it, and (ii) the `FRAUX1` transport grids have no runtime install path at
  all, because `FRPHTAB1` is the only format the seam reads. Nothing calls it
  today; a host that actually takes the offline path will need the seam widened
  to accumulate and to dispatch on the artifact magic. Recorded rather than
  speculatively built.
* **`Air` left the property-diagram picker.** `plot_fluids_available` narrows
  `plot_fluids` by the backend's `served_fluids`, and rustprop's list is Water,
  R134a, R1234yf, `INCOMP::MEG`, `INCOMP::MPG` — no `Air`, because rustprop
  serves the pseudo-pure Air only at `(P,T)`/`(Q,T)`/`(P,Q)` and a diagram needs
  full states. `air.fraux` was the only backing Air had, and it left with the
  other artifacts. Air transport and `Z` at `(T,P)` still answer through
  `props_si`; only the picker entry is gone. Restoring it is a rustprop
  pseudo-pure-flash question, not a frees one.
  **Reversed at Wave-2/Wave-3** — that rustprop question got answered. `Air` is
  back on `served_fluids` and back in the picker; see the D6 amendment at the
  end of this record.
* **Two tolerance files is a real cost.** They can drift. What stops it is that
  each is read in exactly one configuration and both configurations are gated,
  so a stale entry in either fails its own build rather than sitting unnoticed.
* **`frees-cli` gained a `rustprop-backend` passthrough feature**, off by
  default. Without it the accuracy path was reachable only from the wasm module
  and from `cargo test`, and a backend nobody can drive by hand is a backend
  nobody can debug: `frees-cli --features rustprop-backend solve doc.frees` is
  how the staircase in Finding 1 was measured. Default-off keeps a checkout
  without the sibling rustprop tree building the CLI.
* **`HAPropsSI` now answers, and this record does not claim the humid-air
  fixtures.** D8 counted 7 pending fixtures blocked on humid air and 3 on an Air
  state table; rustprop implements the first. Promoting those fixtures is
  measurement work on the pending corpus and is deliberately left outside a
  change whose subject is the switch and the bundle.
* **The `linked-tables` default is load-bearing for the test suite.** Every
  suite that grades an artifact (`props::tables`, `props::auxtable`,
  `props_robustness`, and everything reaching `test_with_builtin_tables`) needs
  the bytes, so `--no-default-features` is not a tested configuration. The
  library compiles without them; the test targets do not, and that is on purpose
  — the alternative is `cfg` noise across five modules to support a
  configuration nothing ships.

## What was explicitly not done

* **The tables were not deleted.** D8 asked for exactly this restraint — "prefer
  selecting a backend over removing one until the size question is answered with
  a measurement". The artifacts, the generators (`tools/table-gen`,
  `tools/aux-gen`), the decoders and `tolerances.json` are all intact, and the
  table configuration is one `--no-default-features` away.
* **The dead tolerance entries were not deleted from `tolerances.json`.** They
  are not dead *there* — they are dead under rustprop. Deleting them would have
  broken the configuration they describe.
* **The engine's stop criterion was not loosened globally**, and rustprop was not
  modified to tighten its flash convergence. Both were on the table for ex16; a
  single declared per-fixture relaxation asserts strictly more than either.
* **No lazy-loading chunk.** D8's third option is unnecessary at 2700 KiB and
  would have to survive the synchronous-solve constraint that
  `props/tables.rs` records against D1. If the budget goes red again it is still
  the largest lever available.

---

## Amendment — Wave-3 D6: the warm adapter's `Air` path is retired

**Status:** implemented
**Date:** 2026-08-18
**Amends** this record's `Air` bullet under *Consequences and open risks*, and
the warm-state adapter Wave-2 F3 added on top of it
(`crates/frees-core/src/props/rustprop_warm.rs`).

Recorded here rather than as `0010-*.md` because it does not decide anything new
— it withdraws half of an existing mechanism whose other half stays exactly as
it was — and because "D6" is this wave's owner-call label, which would collide
with [D6 / `0006-remove-mdf4.md`](0006-remove-mdf4.md) as a file number.

### Decision

`rustprop_warm` no longer claims **pseudo-pure** fluids. It declines them at the
door of `try_props_si`, before either counter moves, and the call reaches
`rustprop::props_si` untouched. The **pure-fluid path is unchanged** — same two
gates, same constants, same cache.

`Air` stays on `RealFluid::served_fluids` and stays in the property-diagram
picker. It is served, just not by the warm path.

### Why — both of F3's justifications moved, one of them all the way

F3 widened the adapter to `Air` for a reason that no longer exists, and kept
water for a reason that shrank but survived.

| | when F3 wrote it | now | verdict |
|---|---|---|---|
| `Air` `(P,Hmass)`/`(P,Smass)` | rustprop: loud `NotImplemented`. The adapter was the **only** way to answer. | rustprop's own pseudo-pure `HSU_P` flash (Wave-2 R6/R7), 5.0 us cold against the adapter's 4.5 us warm | **~1.1x — retire** |
| `Water` and the pure fluids | cold 311-353 us, warm 13-15 us | Wave-2 R8's TOMS748 made cold ~5x faster: cold 60.1-67.2 us, warm 11.8-13.5 us | **~5.2x — keep** |

1.1x does not pay for a hand-rolled locality gate. It pays for even less than
that number suggests, because the gate's *stability* half — the one that makes a
wrong root impossible rather than merely unlikely — cannot function on a
pseudo-pure at all: there is no superancillary, hence no `rho_l(T)`/`rho_v(T)`
to bracket a root with. F3 handled that honestly by claiming only `T > T_crit`
and declining everything below, which for Air means declining below 132.5 K —
so the retired path was a 1.1x speed-up on a partial region of one fluid.

### Verified, not assumed: `Air` is still *correct*, not merely still answering

Deleting the code that computed something is the moment to grade its replacement
against the oracle rather than against itself. Nine `(P, Hmass)` states from
100 K to 800 K and 1 bar to 100 bar, `T` and `Dmass`, against the pinned CoolProp
8.0.0 wheel (`rustprop/tools/golden-gen/.venv`):

**worst relative deviation 1.705e-15 on `T`, 1.695e-15 on `Dmass`** — round-off.
Pinned at 1e-12 in `tests/rustprop_warm.rs::air_p_hmass_matches_the_coolprop_wheel`,
with the wheel's own `T` as the reference rather than the round number the state
was built from (upstream's pseudo-pure flash carries ~1e-9 bracket granularity of
its own — at 150 K it returns 149.999_999_869 — so grading against 150.0 would
grade the wheel, not the port).

A second test, `air_is_served_by_rustprop_and_never_by_the_adapter`, holds the
seam in both directions over six states x two caloric pairs x two input orders x
six outputs: every value is bit-for-bit `rustprop::props_si`'s, the `(P,X)` round
trip lands back on the `(T,P)` temperature, and both adapter counters stay at
zero *even after a deliberate attempt to seed the cache from a neighbouring Air
state* — which is exactly the traffic shape that used to be served warm.

### What was deleted

Not disabled, not left behind a flag: `COLD_MAX_STEPS`, `PSEUDO_T_MARGIN`,
`PSEUDO_T_SEED`, `Start::t_band` and the step clamp it drove, `accept_state`'s
pseudo-pure branch, `cold_serve`'s unseeded ideal-gas Newton, Air's four rows in
the acceptance grid and three in the calibration bases, and the `cold.is_nan()`
escape in the cost assertion that existed only because Air had no cold path to
measure. That is **44 lines of executable code out of `rustprop_warm.rs`**
against 9 added — the `is_pure` predicate and one decline at each of the two
entry points. The module's own line count barely moves (+74/−89) because most of
what replaced the deleted code is the record of why it went.

One consequence is load-bearing and is asserted by construction: past the new
door the superancillary is **unconditional**, so `accept_state` may call
`PtFlash::sat`, which *panics* without one. Both entry points into the module —
`try_props_si` and the `calibration_warm_solve` seam — carry the same decline.

### What did not change

* The pure-fluid gates and their constants. `GATE_LN_P = 0.10`,
  `GATE_DT_REL = 0.01`, `WARM_MAX_STEPS = 4`: re-derived on the Air-free base
  list and **not one rung moved** (pressure still converges fully out to 0.35;
  the caloric axis still breaks between 1e-2 and 2e-2; three steps still fails
  six of the swept states where four fails none). Only the denominators in the
  doc comments changed, 34 -> 28.
* The calibration sweep still runs every build, and still measures the same
  thing: worst accepted deviation 1.141e-13 on `T` and 8.470e-13 on `Dmolar`
  over 672 accepted solves, and nothing outside the gate lies (worst 3.172e-13).
* The warm-vs-cold grid: 30 states instead of 38, every quoted number
  re-measured and unmoved (worst warm 6.063e-14 against cold's 8.441e-10).
* `Water`'s speed-up floor, still 3x against a measured 5.2x.

### Open

`rustprop_warm` now has exactly one customer class — HEOS pure fluids in a
document's Newton loops — and one number justifying it, 5.2x. That number came
down 4x in one wave because rustprop got faster, and rustprop is still getting
faster. The next person to weigh this module should weigh the whole of it, not
another fluid's worth of it.

---

## Amendment — Wave-3 F5: the tolerance list is re-baselined and re-labelled

**Status:** implemented
**Date:** 2026-08-18
**Amends** this record's *What it clears* section and
`fixtures/tolerances-rustprop.json` in full.

Recorded here rather than as a new ADR for the same reason D6 was: it decides
nothing new. It re-measures a number this decision already asserted, and
corrects the two places where that assertion had drifted from what the corpus
actually does.

### The question

D9 adopted rustprop and, in the same change, deleted the twelve tolerance
entries that the accuracy path made dead. It did **not** re-derive the ones that
stayed; it inherited their reasons from D1-era analysis and reasoned about them.
That left an open question worth a task of its own: **of the entries that
survived a perfect CoolProp, which are real and which were artifacts of the old
backend's error masking something else?**

The trap was named in advance: not every entry dies with a perfect port, because
some *goldens are themselves wrong*. Widening a band to accommodate reference
error and widening one to accommodate port error look identical in the file and
mean opposite things.

### What was measured

Every one of the 707 fixtures was replayed and its worst variable recorded, then
each surviving entry was traced to its leaf property call and that call was put
to a **third oracle** — the CoolProp 8.0.0 wheel in
`../rustprop/tools/golden-gen/.venv`, queried at the fixture's own inputs. Three
values per leaf: what the golden says, what rustprop says, what CoolProp says.

The result is unambiguous, and it is stronger than what D9 claimed:

| | entries | leaf: rustprop vs CoolProp 8.0.0 | leaf: golden vs CoolProp 8.0.0 |
|---|---:|---|---|
| Java `(P,h)` table | 9 | 0 … 2.1e-13 (bit-identical in three) | 2.3e-7 … 6.7e-6 |
| upstream flash residual | 1 | 2.9e-15 (self-consistent) | 1.6e-10 |

**No entry in the file is port error.** At every leaf that feeds a surviving
tolerance, rustprop is nearer to CoolProp than the golden is — by four to nine
orders of magnitude, and in three cases bit-identical to it.

The sharpest of these is `props_realfluid_r134a_states`, a flat property matrix
with literal inputs, so **all 42 of its variables** were checked against the
wheel one for one: its **24 table-shape variables** ((P,h) → T/Dmass/Smass) miss
the wheel by 7.8e-10…1.8e-6, while **all 18 non-table variables are
bit-identical to it** — including `Quality` and `IntEnergy` *at the same
(P, h)*, which `PhTableRegistry.TABLE_OUTPUTS` excludes by name. The dividing
line in the data is exactly the dividing line in the Java source.
`props_realfluid_water_states` repeats it over all 49 of its variables with one
extra wrinkle: 27 of its 28 non-table variables are bit-identical and the 28th
is `Volume`, which the Java reports as `1/Dmass` off the tabulated density.

### What changed in the file

* **Nothing was deleted.** The dead-tolerance guard flagged no entry: all ten
  fixtures still exceed the 1e-9 default. (This is the third consecutive change
  to that file in which the guard, not a human, decided what could go.)
* **Every `relative` is now `measured × 1.5`, to two significant figures**,
  replacing hand-picked round numbers. The nine table entries are frozen by
  construction — a committed golden minus a value rustprop reproduces to
  2.2e-13 or better is arithmetic on two constants — so a tight band there costs
  nothing and any movement is a rustprop regression.
* **`mechanism` is now a machine-checked slug**, `oracle-ph-table` or
  `upstream-ps-flash-residual`, defined in a new top-level `mechanisms`
  catalogue, and each `reason` carries the three-way numbers for *its own*
  fixture rather than pointing at a shared prose paragraph.
* **The catalogue is guarded in both directions.** An entry naming an undefined
  slug fails; a defined slug that no entry names *also* fails. That second guard
  exists because this file had already grown a dead explanation — the retired
  stop-criterion mechanism, kept as an empty section with a live-sounding
  description for a whole wave after its last instance converged at the default.
  All three directions were verified by perturbing the file and watching the
  gate go red.

### The one entry that is not the Java's table

`refrigeration-vcr` is the only fixture in the corpus with no table shape
anywhere in it — `P_sat`, `(T,x)`, `(P,x)` and `(P,s)` all fall through to the
native library — so D9 reasoned that its residual had to be cancellation of
bit-exact values in the COP. It is not. `p1`, `p2`, `s1` and `t_evap` *are*
bit-identical, but the leaf `h2s = Enthalpy(R134a, P = p2, s = s1)` is 1.6e-10
away on its own, before any subtraction.

The cause is upstream. A fresh
`AbstractState('HEOS','R134a').update(PSmass_INPUTS, p2, s1)` converges to a
state whose **own pressure is 4.06e-9 relative from the pressure that was
asked for**, and reports `hmass` evaluated there. Ask the wheel for `hmass` at
its own converged `(T, rho)` and it answers 426479.692_669_698_04; ask it
through the flash and it answers 426479.692_738_112. rustprop returns
426479.692_669_696_8 — the first, to 2.9e-15, on a state that reproduces *both*
inputs to ~1e-15.

So the golden is reference-side here too, just tainted by CoolProp rather than
by the Java. But it is the one entry that is **not frozen**, and it points two
ways at once: a port that is bitwise faithful to upstream would reproduce the
residual and this fixture would go to zero. Either direction of travel makes the
entry fail — a regression widens it, a fidelity fix kills it — which is the
correct behaviour for both, and the reason the entry says in writing not to
widen it in place.

Probed across four superheated R134a states the same residual is 4.06e-9,
7.31e-12, 1.16e-16 and 4.37e-16: a stopping criterion, not a systematic offset,
which is why exactly one fixture in 707 shows it.

### The prediction that did not come true

The investigation flagged `props_realfluid_r134a_states` as the entry most
likely to **newly fail** under an accurate backend — 24 of its 42 variables are
table-shape, so a port that stopped hiding behind the old error might overshoot
the band. It does not fail. It measures 1.752_325e-6 against a 2.7e-6 band, and
its worst variable `t_h2s` reads 319.438_780_796_629_9 on rustprop against the
wheel's 319.438_780_796_629_8 — one ulp — while the golden reads
319.439_340_558_266_05. The fixture's entire error is on the golden side, to
five significant figures.

### Open

The nine table entries can only be retired by re-dumping their goldens from a
Java oracle with `PhTableRegistry` disabled. That is a change to
`../frees/backend/core`, not to this repo, and it would delete nine of the ten
remaining accuracy exceptions at a stroke. Whether that is worth doing depends
on whether the Java engine is still a reference this project intends to keep.

## Amendment — Wave-3 F6: the fixtures D8 predicted are promoted

*2026-08-18.* "Consequences and open risks" above says, of the humid-air
fixtures, that *"promoting those fixtures is measurement work on the pending
corpus and is deliberately left outside a change whose subject is the switch and
the bundle."* That measurement is done.

### What was measured

Each pending document in the three groups
[D8](0008-coolprop-wasm.md) named — humid air (7), `(P,T)` transport off the
dome (`hx-correlations-fluid`), and `CompressibilityFactor`
(`thermo-compliance`) — was replayed with `tests/parity.rs`'s own comparison
logic pointed at `fixtures/corpus-pending/golden` and at **no** tolerance file,
so the grade is the corpus default `1e-9` and no per-fixture exception exists to
fall back on. All nine clear:

| Document | Worst variable deviation |
|---|---:|
| `adv_moistair_W_passthrough` | 0 |
| `adv_moistair_dryair_three_way` | 0 |
| `hvac-problem9-air-supply-wet-bulb` | 0 |
| `sysdesign-ex12-moist-air-ahu` | 0 |
| `sysdesign-ex13-humidifier` | 0 |
| `hvac-problem3-psychrometric-balancing` | 1.25e-15 |
| `hvac-problem2-face-and-bypass` | 2.29e-15 |
| `hx-correlations-fluid` | 1.24e-13 |
| `thermo-compliance` | 1.31e-11 |

A `0` is the harness's `rel_diff` returning zero inside its `1e-12` absolute
band — on a psychrometric enthalpy of order 5e4 J/kg that is the last few bits,
not a rounded report. Corpus **707 → 716**, pending **26 → 17**, and
`fixtures/tolerances-rustprop.json` is untouched: nine documents entered the
gate and the file still carries exactly the ten entries F5 re-baselined.

### D8's twelfth, eleventh and tenth also clear

The one group F6 did not promote — the `Air` **state**-table three,
`sysdesign-ex06-pneumatic`, `sysdesign-ex06-pneumatic-2` and
`sysdesign-ex07-pneumatic-servo` — was replayed under the same rules and grades
at 1.43e-14, 0 and 0. D8's prediction was *twelve of the 26*; the measurement
says twelve of twelve. They are staged rather than promoted only because F6's
scope was the nine, and promoting them is a file move plus a table edit
(pending 17 → 14).

### What this does *not* close

The remaining twelve holds were re-measured in the same run and **none is a
property blocker**: five `linalg::svd` column-sign documents plus
`estimator-gramian-balreal` (sign-only element flips, relative deviation ~2.0 by
construction), three `eqsys-*` awaiting `CALL eigenvalues`/`eigen`,
`module_inside_for_loop` (pipeline ordering), `pressure-cooker`
(`method = ida`), and `sysdesign-ex01-thermal-network-2` (a signal decaying
through zero, so the relative measure has no denominator).

The `HAPropsSI` deviation D9 already records stands — rustprop returns humid-air
errors as a `Result` where upstream returns `+inf` with a global error string.
Nothing in these nine documents reaches an error state, so the promotion neither
tests nor closes it.
