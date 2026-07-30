# `tools/table-gen` — CoolProp property-table generator

Generates the precomputed real-fluid property tables that decision **D1**
(`docs/decisions/0001-property-backend.md`) picks as the browser build's hot
path, and **measures the error they introduce** so that choice rests on numbers.

Sibling of `tools/golden-dumper`: same `classpath.sh` (delegated, not copied),
same discipline of adding no dependency the reference engine does not already
carry, same hand-rolled JSON so no Jackson version has to be reconciled on an
already-crowded classpath. It reads `../frEES` and never writes to it.

```bash
./run.sh                                    # Water + R134a -> fixtures/proptables/
./run.sh /tmp/tables --sweep --samples 1500 # resolution ladder, writes no tables
./run.sh /tmp/tables --fluids R1234yf,CO2   # more fluids
./run.sh --help                             # (any invalid flag prints the options)
```

`run.sh` exports `COOLPROP_LIBRARY` the same way the golden dumper does and
**fails** rather than emitting a table if the native library is missing — every
number in the output comes from CoolProp, so a silent fallback would produce a
file full of nothing.

## Where the design comes from

The generator does not invent a scheme. The reference engine already contains
the exact architecture, dormant behind a feature gate:

| Reference file | Role | Rust stub |
|---|---|---|
| `props/PhPropertyTable.java` | Bicubic Hermite `(P, y)` surface with analytic ∂/∂P, ∂/∂h | `props/phtable.rs` |
| `props/SaturationSplitTable.java` | Phase split: sat lines + two-phase exact relations + two single-phase pieces | `props/satsplit.rs` |
| `props/PhTableRegistry.java` | Lazy per-fluid registry, 1e-4 validation gate, fallthrough to native | — |

This tool's job is to move the *data* those classes build at runtime — from
native CoolProp calls that do not exist in wasm — to disk at build time.
So the on-disk format is a serialization of exactly what
`SaturationSplitTable` holds, and the Rust side is a straight port of the two
Java classes plus a reader.

**Why `TableGen` declares `package com.frees.backend.props`.** Not convenience —
provability. In that package the tool can

* build its grids with the **real** `PhPropertyTable`, so the interpolation
  error it reports is the error of the code the Rust port transcribes, not of a
  lookalike written for the measurement; and
* cross-check its (necessarily resolution-parametric, because
  `SaturationSplitTable` hard-codes 256/96/48) restatement of the split geometry
  against the real `SaturationSplitTable`, so a transcription slip fails loudly
  instead of producing a quietly wrong table. `MANIFEST.json` records the worst
  relative disagreement as `cross_check_vs_reference_max_rel`.

## Geometry

Everything is SI, matching the rest of frees.

* **Saturation lines**, `n_sat` samples uniform in `ln P` over
  `[p_min, p_max]`, where `p_min = max(1.2·p_triple, 1e-4·p_crit)` and
  `p_max = 0.75·p_crit`. Interpolated with cubic Hermite on `ln P` and
  central-difference slopes.
* **Two-phase region**, `h_f(P) ≤ h ≤ h_g(P)`: exact mixture relations off those
  lines — `T = T_sat(P)`, `v = v_f + x·v_fg`, `s = s_f + x·s_fg`,
  `x = (h − h_f)/(h_g − h_f)`. No 2-D surface ever crosses the dome.
* **Superheated vapor**: bicubic over `(P, y)` with `y = h − h_g(P)`, so the grid
  follows the dome instead of cutting across it. `P` log-spaced over
  `[p_min, p_max]`; `y` **quadratically** spaced over `[0, dh_vapor_max/0.9]`, so
  nodes crowd where the curvature is — at the dome edge.
* **Subcooled liquid**: bicubic over `(P, y)` on the same `P` spacing, from
  `p_liquid_min` up. Two coordinate choices, selected by `--liquid`:

  | mode | `y` | coverage |
  |---|---|---|
  | `absolute` | `h_f(P) − h`, capped at one depth valid at *every* pressure | the reference geometry; bounded by the thinnest sliver (low `P`), so cold high-pressure liquid falls out of the table |
  | `normalized` *(default)* | `(h_f(P) − h) / (h_f(P) − h_cold(P))` ∈ [0, 1], `h_cold(P) = h(P, T_low)` | the whole liquid sliver at every pressure |

  `absolute` is what `SaturationSplitTable` does, and it is fine there because
  an uncovered point falls through to a native call. In the browser there is
  nothing to fall through to — see D1 for the measured coverage difference.
* **Serve limits.** Lookups return "uncovered" outside `[p_min, p_serve_max]`
  (`p_serve_max = 0.95·p_max`) or beyond `dh_vapor_max` / `dh_liquid_max`. The
  fits extend past the served edge on purpose: the last cells before a grid
  boundary use one-sided derivative stencils and carry the worst error, so the
  served region keeps a margin of fitted-but-unserved cells.

Tabulated outputs, in plane order: **`T`** (K), **`Dmass`** (kg/m³),
**`Smass`** (J/kg·K) — the same three `PhTableRegistry` tabulates. Quality is
excluded deliberately (piecewise, and `−1` outside the dome breaks any
interpolation); specific volume is `1/Dmass`; internal energy is `h − P/Dmass`.

Non-finite nodes (CoolProp can return garbage right at the critical point) are
replaced by their nearest finite neighbour **before serialization**, by the same
expanding-ring scan `PhPropertyTable.fillNonFinite` uses. The reader therefore
needs no NaN handling at all, and cannot diverge from the oracle by tie-breaking
that scan differently. `MANIFEST.json` records how many nodes were repaired.

## On-disk format `FRPHTAB1`

One file per fluid, `<fluid>.phtab`, lowercase name. **All integers and floats
little-endian** (wasm and x86 agree; no byte-swapping on any target that
matters). Nothing is compressed — gzip on the wire is the transport's job.

### Header

| Offset | Size | Type | Field |
|---:|---:|---|---|
| 0 | 8 | `u8[8]` | magic, ASCII `FRPHTAB1` |
| 8 | 2 | `u16` | `format_version` = 1 |
| 10 | 1 | `u8` | `elem_kind`: 0 = `f64`, 1 = `f32` (payload only) |
| 11 | 1 | `u8` | `flags`: bit 0 = liquid piece present, bit 1 = liquid `y` is normalized |
| 12 | 4 | `u32` | `n_sat` — saturation-line samples |
| 16 | 4 | `u32` | `n_p` — pressure nodes per 2-D piece |
| 20 | 4 | `u32` | `n_dh` — depth nodes per 2-D piece |
| 24 | 4 | `u32` | `n_props` = 3 |
| 28 | 4 | `u32` | `header_bytes` — byte offset of the payload |
| 32 | 8 | `f64` | `p_min` [Pa] |
| 40 | 8 | `f64` | `p_max` [Pa] |
| 48 | 8 | `f64` | `p_serve_max` [Pa] |
| 56 | 8 | `f64` | `p_liquid_min` [Pa] (`+inf` when there is no liquid piece) |
| 64 | 8 | `f64` | `dh_vapor_max` [J/kg] |
| 72 | 8 | `f64` | `dh_liquid_max` — [J/kg] when absolute, dimensionless (0.9) when normalized |
| 80 | 8 | `f64` | `h_top` [J/kg] — the enthalpy ceiling the vapor piece was sized from |
| 88 | 8 | `f64` | `t_low` [K] — the cold end the liquid piece was sized from |
| 96 | 8 | `f64` | `p_crit` [Pa] |
| 104 | 8 | `f64` | `t_crit` [K] |
| 112 | 8 | `f64` | `p_triple` [Pa] |
| 120 | 8 | `f64` | `t_triple` [K] |
| 128 | 2 | `u16` | `fluid_len` |
| 130 | 2 | `u16` | `coolprop_version_len` |
| 132 | 4 | `u32` | `backfilled_nodes` |
| 136 | — | `u8[]` | fluid name (UTF-8), then CoolProp version (UTF-8), then zero padding to a multiple of 8 |

The header scalars are **always `f64`** regardless of `elem_kind`; they define
the grid geometry, and rounding them would move cell boundaries.

### Payload

Element type is `elem_kind` throughout. Every array is contiguous, in this
order:

```
saturation block — 9 arrays of n_sat, in order:
    log_p, t_sat, h_f, h_g, v_f, v_fg, s_f, s_fg, h_cold

vapor piece:
    p_grid  [n_p]                 strictly increasing, log-spaced
    y_grid  [n_dh]                strictly increasing, quadratically spaced, y_grid[0] = 0
    T       [n_p * n_dh]          row-major: index = i*n_dh + j  (i over P, j over y)
    Dmass   [n_p * n_dh]
    Smass   [n_p * n_dh]

liquid piece — present iff flags bit 0:
    p_grid  [n_p]
    y_grid  [n_dh]
    T, Dmass, Smass                same layout
```

`h_cold` is written unconditionally, even in `absolute` mode, because it is what
lets a reader convert between the two liquid coordinates and bound the liquid
sliver.

The grids are stored explicitly rather than recomputed from `logspace` /
`squarespace` formulas: the reader then uses the *same* numbers for cell
location and for interpolation, and cannot drift from the generator by a
floating-point hair.

### Reading it

The payload is intentionally *not* zero-copy castable — a `Vec<u8>` from `fetch`
is 1-byte aligned, and casting it to `&[f64]` needs `unsafe`, which this port
does not use. Read with `f64::from_le_bytes` / `f32::from_le_bytes` into owned
`Vec<f64>` once at load time; the cost is a few hundred microseconds, paid once.

Reader checklist:

1. Check magic and `format_version`; reject anything else.
2. Seek to `header_bytes` — do not assume 136 or 144; the string block is
   variable-length.
3. Read the nine saturation arrays, then the vapor piece, then the liquid piece
   if `flags & 1`.
4. Feed each `n_p × n_dh` plane to the `PhPropertyTable` port's `build`
   equivalent along with its `p_grid` / `y_grid`. Node values are already
   back-filled; nodal derivatives are finite-differenced at load time exactly as
   `PhPropertyTable.build` does, which is what makes the surface C¹.

### Sizes

`n_sat = 256`, `n_p = 96`, `n_dh = 48`, both pieces present, `f64`:

```
header                                            ~152 B
saturation   9 × 256 × 8                        18,432 B
vapor        (96 + 48 + 3×96×48) × 8             111,744 B
liquid       (96 + 48 + 3×96×48) × 8             111,744 B
                                                --------
                                               ~242,072 B  (~118 KB with --f32)
```

Per fluid. Measured sizes for the shipped grid are in
`docs/decisions/0001-property-backend.md` and in `MANIFEST.json`.

## Outputs

| File | Contents |
|---|---|
| `<fluid>.phtab` | the table, format above |
| `MANIFEST.json` | per fluid: SHA-256, byte size, CoolProp version, resolution, every grid bound, back-fill count, CoolProp call count, cross-check result |
| `ERROR-REPORT.json` | the measured tabulation error (see below) |
| `SWEEP.json` | `--sweep` only: one row per (fluid, liquid mode, resolution) with size and error |

## What the error measurement actually measures

At a fixed seed, `--samples` points are drawn with `P` log-uniform on
`[p_min, p_serve_max]` and `h` uniform on `[h_f(P) − 150 kJ/kg,
h_g(P) + 500 kJ/kg]` — the same band `PhTableRegistry`'s own validation gate
uses, so the numbers are comparable to the reference's 1e-4 threshold. For each
point the table is compared against a direct CoolProp call:

* **forward** `(P,h) → T, Dmass, Smass`, max and RMS relative error, and broken
  down by region (subcooled / two-phase / superheated), plus the fraction of
  samples the table covers at all;
* **inverse** `(P,T) → h` and `(P,s) → h` by bisection on the tabulated surface.
  These matter more than the forward form for this port: `PhTableRegistry` only
  ever intercepts `(P, Hmass)` inputs, so the reference never exercises an
  inversion — but `Enthalpy(Water, P=…, T=…)` and `Enthalpy(Water, P=…, s=…)`
  are exactly what the pending Rankine and refrigeration fixtures call;
* **saturation** `(P,x) → h` off the lines, and `(T,x)` after inverting
  `T_sat(ln P)`;
* **document states** — the concrete `(P, h)` pairs the pending fluid fixtures
  land on, lifted from `fixtures/corpus-pending/golden/`, reported individually
  so a regression shows up on the states that are actually gated rather than
  only in an aggregate.

Relative error uses `|tab − direct| / max(|direct|, 1e-12)`. Entropy is
reported this way too, but note that its reference state is arbitrary
(`s = 0` at the R134a saturated-liquid reference point), so relative entropy
error is inflated wherever `s` happens to pass through zero — read the absolute
value alongside it.

## Limits, stated rather than discovered later

* **Subcritical only.** `p_max = 0.75·p_crit`, so nothing supercritical is
  covered. `PhPropertyTable`'s doc comment advertises supercritical support;
  `SaturationSplitTable`'s geometry does not use it. A transcritical CO₂ or R744
  cycle needs a fourth region that neither this tool nor the reference builds.
* **No humid air.** `HAPropsSI` is a three-input function on a different
  manifold and is not tabulated here.
* **No transport properties.** Viscosity and conductivity are rare enough that
  the reference leaves them on the direct path; they are not in the plane set.
* **Quality is derived, not stored** — `x = (h − h_f)/(h_g − h_f)` inside the
  dome, undefined outside.
* **Mixtures and incompressibles** (`INCOMP::MEG-50%`, the `EG50` in
  `ev-thermal-management`) have no dome and are not handled by this geometry.
