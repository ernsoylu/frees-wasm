---
name: heisler_q
category: Heat Transfer
summary: Fraction of total heat transferred Q/Q0 (Heisler one-term) for a wall, cylinder, or sphere.
related: [heisler_temp]
examples: [heisler-transient]
tags: [transient conduction, heisler, heat fraction, biot, fourier, energy]
---

# heisler_q

Returns the **fraction of the maximum possible heat** `Q/Q0` that a plane wall,
infinite cylinder, or sphere has exchanged with its surroundings up to Fourier
number `Fo`, using the one-term (Heisler) approximation. `Q0 = ρ·c·V·(Ti − T∞)` is
the energy available relative to the ambient.

## Syntax

```
ratio = heisler_q(geom$, Bi, Fo)
```

## Description

While `heisler_temp` gives a point temperature, `heisler_q` gives
the integrated energy removed (or added) so far — useful for transient duty and
storage calculations.

## Mathematical Formulation

With the midplane ratio $\theta_0^* = C_1\exp(-\lambda_1^2 Fo)$ and
$Q_0 = \rho c V(T_i - T_\infty)$:

$$ \text{wall: } \frac{Q}{Q_0} = 1 - \frac{\theta_0^*}{\lambda_1}\sin\lambda_1 $$
$$ \text{cylinder: } \frac{Q}{Q_0} = 1 - \frac{2\theta_0^*}{\lambda_1}J_1(\lambda_1), \qquad \text{sphere: } \frac{Q}{Q_0} = 1 - \frac{3\theta_0^*}{\lambda_1^3}\big(\sin\lambda_1 - \lambda_1\cos\lambda_1\big) $$

> **Method:** first-term truncation using the same `λ1(Bi)`, `C1(Bi)` as
> `heisler_temp`.

## Examples

### Example 1 — Heat removed from a cooling plate

[Run: heisler-transient]

**Expected (approx.):** for the plane wall at `Bi = 3.33`, `Fo = 0.225`,
`heisler_q ≈ 0.33` (about a third of the removable heat has left).

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `geom$` | String | Yes | Geometry: `'wall'`, `'cylinder'`, or `'sphere'`. |
| `Bi` | Number | Yes | Biot number `h·s/k`. |
| `Fo` | Number | Yes | Fourier number `α·t/s²` (one-term valid for `Fo > 0.2`). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `ratio` | Number | Heat fraction Q/Q0 ∈ [0, 1]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `UNKNOWN_GEOMETRY` | `geom$` not recognized | Use `'wall'`, `'cylinder'`, or `'sphere'`. |
| (inaccurate result) | `Fo < 0.2` | The one-term approximation is invalid very early in the transient. |
