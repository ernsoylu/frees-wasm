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
