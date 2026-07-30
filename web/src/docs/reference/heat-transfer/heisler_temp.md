---
name: heisler_temp
category: Heat Transfer
summary: One-term (Heisler) transient temperature ratio for a wall, cylinder, or sphere.
related: [heisler_q]
examples: [heisler-transient]
tags: [transient conduction, heisler, biot, fourier, one-term, unsteady]
---

# heisler_temp

Returns the **dimensionless temperature** `θ* = (T − T∞)/(Ti − T∞)` at a point in a
plane wall, infinite cylinder, or sphere undergoing 1-D transient conduction with
surface convection — the one-term (Heisler) approximation, valid for Fourier number
`Fo > 0.2`. Use it when the Biot number is large enough that lumped capacitance
fails and internal gradients matter.

## Syntax

```
theta = heisler_temp(geom$, Bi, Fo, xstar)
```

## Description

`geom$` selects the geometry (`'wall'`, `'cylinder'`, `'sphere'`); `Bi = h·s/k` and
`Fo = α·t/s²` use the characteristic length `s` (half-thickness `L` for a wall,
radius `r0` for a cylinder/sphere). `xstar` is the dimensionless position (`0` =
centre/midplane, `1` = surface). Recover the temperature with
`T = T∞ + θ*·(Ti − T∞)`.

## Mathematical Formulation

Midplane/centre temperature (`Fo > 0.2`):

$$ \theta_0^* = C_1\,\exp\!\left(-\lambda_1^2\,Fo\right) $$

Position correction `θ*/θ_0*`:

$$ \text{wall: }\cos\!\left(\lambda_1 x^*\right),\quad \text{cylinder: }J_0\!\left(\lambda_1 x^*\right),\quad \text{sphere: }\frac{\sin(\lambda_1 x^*)}{\lambda_1 x^*} $$

where $\lambda_1(Bi)$ and $C_1(Bi)$ are the first-eigenvalue coefficients for the
geometry, with $Bi = hs/k$ and $Fo = \alpha t/s^2$.

> **Method:** first-term series truncation; the eigenvalue `λ1` and coefficient
> `C1` are evaluated for `Bi` and the selected geometry.

## Examples

### Example 1 — Centre and surface temperature of a cooling plate

A plane wall (`Bi = 3.33`, `Fo = 0.225`) cooling from 200 °C into a 25 °C stream.

[Run: heisler-transient]

**Expected (approx.):** `θ_c ≈ 0.87` → `T_centre ≈ 177 °C`; `θ_s ≈ 0.30` →
`T_surface ≈ 77 °C`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `geom$` | String | Yes | Geometry: `'wall'`, `'cylinder'`, or `'sphere'`. |
| `Bi` | Number | Yes | Biot number `h·s/k`. |
| `Fo` | Number | Yes | Fourier number `α·t/s²` (one-term valid for `Fo > 0.2`). |
| `xstar` | Number | Yes | Dimensionless position: `0` centre, `1` surface. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `theta` | Number | Dimensionless temperature θ* = (T − T∞)/(Ti − T∞). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `UNKNOWN_GEOMETRY` | `geom$` not recognized | Use `'wall'`, `'cylinder'`, or `'sphere'`. |
| (inaccurate result) | `Fo < 0.2` | The one-term approximation is invalid early in the transient; the centre has barely responded. |
