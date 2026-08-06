# `tools/aux-gen` — the `FRAUX1` auxiliary property grids

`tools/table-gen` tabulates a fluid *with a saturation dome* in `(P,h)`. This
tool tabulates the three surfaces that geometry deliberately has no room for,
and which between them were the whole of the "real-fluid coverage the tables do
not have" divergence:

| Kind | Serves | Why the `(P,h)` split table cannot |
|---|---|---|
| `INCOMPRESSIBLE` | `INCOMP::MEG[x]`, `INCOMP::MPG[x]` — the aqueous glycols | They have no dome at all. D1's geometry is built out of one. |
| `PRESSURE_TEMPERATURE` | `Air` transport, for `htc_extair` | Air is not tabulated at all, and the split table stores no transport. |
| `SATURATION_LINE` | `viscosity` / `conductivity` / `Cpmass` at `Q=0` and `Q=1` | The split table stores `T`, `Dmass`, `Smass` and nothing else. |

```bash
./run.sh                          # every grid, into fixtures/auxtables
./run.sh /tmp/aux --only MEG      # one family
./run.sh --sweep                  # error-vs-resolution ladder, writes nothing
```

`COOLPROP_LIBRARY` is resolved the same way `tools/table-gen` resolves it, via
`tools/frees-home.sh`.

## The two decisions that set the accuracy

**The concentration axis is at exactly 1 % steps, and that is a correctness
decision, not a resolution one.** `PropertyFunctions.resolveFluid` can only ever
produce a two-decimal mass fraction — it parses an integer percent and formats
it as `String.format("0.%02d")`, so `EG50` is `0.50` and the language has no
spelling for `0.505`. Putting a node on every value a document can name makes
the concentration lookup an *exact hit* rather than an interpolation.

That is not a micro-optimisation. A first cut used 25 columns over `[0, 0.6]`
and measured viscosity at `3.2e-2` — and refining only as `1/n_x`, first-order,
the signature of interpolating across the nonlinear freeze curve rather than of
a smooth surface under-resolved. Moving to exact nodes dropped the same
measurement to `1.4e-3` and restored clean second-order convergence in the
remaining axis.

**The saturation axis stops at `0.75 · p_crit`, the same ceiling `TableGen`
uses.** Approaching the critical point `cp` and `conductivity` diverge, and no
tractable grid interpolates a divergence: running the axis to `p_crit` measured
`max_rel = 2.0e+01` on water's `Cpmass`, all of it in the last few nodes.
Matching the split table's ceiling removes that region — and removes nothing a
caller can reach, since the split table cannot produce a state above its own
`p_serve_max` either.

The normalized-temperature axis of the incompressible grids is **not uniform**:
`errorEquidistributed` places its nodes so that the piecewise-linear
interpolation error of `ln(mu)` is level across the band. Every output on that
surface is gentle except viscosity, which follows an Arrhenius-ish
`ln(mu) ~ 1/T` and turns over hard near the freeze point, so that one output
would otherwise set the grid size for all eight.

Linear-interpolation error goes as `h²·|f''|`, so the node density that levels it
is proportional to `sqrt(|f''|)`; the tool measures `|f''|` from the real
`ln(mu)` surface, averages it over the concentration columns, and inverts the
cumulative density at `ntau` equal quantiles. A first attempt used a guessed
`tau = u²` instead, which fixed the cold end and then over-corrected — it left
the warm end coarser than uniform and the error simply moved there. The density
is floored at a third of its mean so that cannot happen again. All of this costs
**zero extra bytes**: the axis is written out explicitly and the reader binary-
searches it.

## Measured error (`AUX-ERROR-REPORT.json`, CoolProp 8.0.0)

Worst `max_rel` over 4000 random interior states per grid:

| Grid | bytes | `Dmass` | `Cpmass` | `viscosity` | `conductivity` |
|---|---|---|---|---|---|
| `INCOMP::MEG` 61×48 | 94 820 | 1.6e-5 | 9.5e-5 | 1.3e-3 | 6.6e-5 |
| `INCOMP::MPG` 61×48 | 94 820 | 1.5e-5 | 1.4e-5 | 9.1e-4 | 1.4e-4 |
| `Air` 24×64 | 25 072 | 1.9e-4 | 2.2e-4 | 1.2e-4 | 1.4e-4 |
| `Water` sat 512×2 | 14 480 | — | 8.2e-4 | 6.5e-5 | 4.1e-4 |
| `R134a` sat 512×2 | 14 480 | — | 5.3e-4 | 7.0e-5 | 1.7e-4 |
| `R1234yf` sat 512×2 | 14 488 | — | 1.6e-3 | 2.2e-4 | 6.2e-4 |

**Viscosity and saturated `Cpmass` are the two outliers**, and both are stated
rather than smoothed over. They feed `htc_1phase` / `htc_evap` / `htc_cond` /
`dp_2phase`, whose own Nusselt and two-phase-multiplier correlations are
±20 %-accurate by construction; a 1.6e-3 property is three orders of magnitude
inside the correlation's own error. Everything a state calculation uses directly
— `Dmass`, `Hmass`, `Smass` — is at or below `2.2e-4`, i.e. D1's own class.

There is one place that error gets amplified, and it is worth knowing about
before reading a parity tolerance:
`htc_1phase('EG50', 200 kPa, 290 K, 0.23, 0.008, 1.5e-4)` — the call
`ev-thermal-management` makes — runs at **`Re = 2987`, dead centre of
`nuSinglePhase`'s 2300..4000 laminar↔turbulent blend**, where Nu sweeps
3.66 → ~30. A ~5e-4 viscosity error landing there comes out near 9e-4 in `h`.
That gain belongs to the operating point, not the grid: the same grid grades
`sysdesign-ex11-liquid-cooling-loop`, which sits off the blend, at 1.3e-4.

`Hmass` and `Smass` carry a large *pointwise* `max_rel` in the report and a small
`max_rel_scaled`. That is a zero-crossing artifact, not error: CoolProp's
incompressible reference puts glycol `h = 0` near 293 K, so samples a few kelvin
away divide a small absolute error by a near-zero reference. Read
`max_rel_scaled` there. D1's ERROR-REPORT has the same artifact on water entropy
near the triple point.

## Why an incompressible grid is exact in pressure

CoolProp's incompressible model makes `rho`, `cp`, `mu` and `k` **exactly**
pressure-independent, and `h` and `s` **exactly linear** in pressure. Measured,
not assumed — `dh/dP` at 305 K reproduces to all 16 digits from 1 bar to 100 bar,
and `AuxGen` re-checks the linearity at a third pressure at every node and
*fails the run* rather than writing a table that quietly is not the library.

So the grid stores `h` and `s` at a reference pressure plus their (constant)
pressure slopes, and the reader reconstructs

```text
h(P,T) = h_ref(x,tau) + dHmass_dP(x,tau) * (P - P_ref)
s(P,T) = s_ref(x,tau) + dSmass_dP(x,tau) * (P - P_ref)
```

with **no error beyond the `(x, tau)` interpolation itself**. `P_ref` is written
into the header rather than agreed by convention, so a reader cannot silently
assume the wrong one.

## Binary format

All integers little-endian; all arrays `f32` or `f64` per `elem_kind`.

```text
off  size  field
  0     8  magic "FRAUX1\0\0"
  8     2  u16 format_version = 1
 10     1  u8  elem_kind (0 = f64, 1 = f32)
 11     1  u8  flags — bit 0 RAGGED (axis 2 is normalized, per-column endpoints)
 12     4  u32 kind (0 = incompressible, 1 = pressure_temperature, 2 = saturation_line)
 16     4  u32 n1        (axis-1 samples)
 20     4  u32 n2        (axis-2 samples)
 24     4  u32 n_outputs
 28     4  u32 header_bytes  (payload starts here; 8-aligned)
 32     8  f64 axis1_min
 40     8  f64 axis1_max
 48     8  f64 axis2_min
 56     8  f64 axis2_max
 64     8  f64 ref_pressure  (INCOMPRESSIBLE only; 0 otherwise)
 72     2  u16 name_len
 74     2  u16 coolprop_version_len
 76     2  u16 axis1_name_len
 78     2  u16 axis2_name_len
 80     4  u32 reserved (0)
 84   ...  name, coolprop version, axis-1 name, axis-2 name (UTF-8, in that order)
      ...  per output: u16 name_len, name bytes, u8 transform (0 = linear, 1 = log)
      ...  zero padding to header_bytes

payload:
      n1  axis1 values   (log P where the axis is named log_P)
      n2  axis2 values
      n1  axis2_lo  ) RAGGED only — the real axis-2 endpoints of column i,
      n1  axis2_hi  ) so the stored axis-2 value is (v - lo) / (hi - lo)
   n1*n2  output plane, axis-1 outer / axis-2 inner, one per output in
          declaration order, stored under that output's transform
```

A `log` transform means the *stored* number is `ln(value)`; the reader
interpolates in that space and exponentiates. Viscosity spans decades, and
interpolating it linearly is what makes an otherwise-fine grid bad.
