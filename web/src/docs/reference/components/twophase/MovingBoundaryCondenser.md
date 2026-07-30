---
name: MovingBoundaryCondenser
category: Component (twophase)
summary: A moving-boundary condenser tracking the two-phase/subcooled zone lengths.
related: []
examples: []
tags: [movingboundarycondenser, component, twophase, acausal]
---

# MovingBoundaryCondenser

A moving-boundary condenser tracking the two-phase/subcooled zone lengths.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`, `wall`

## Usage

```
MovingBoundaryCondenser inst(fluid$, U_cond, U_sc, D, L, eps_zone, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `U_cond` | Number | Condenser-zone overall coefficient [W/m²·K]. |
| `U_sc` | Number | Subcool-zone overall coefficient [W/m²·K]. |
| `D` | Number | Diameter [m]. |
| `L` | Number | Length [m]. |
| `eps_zone` | Number | Zone-collapse smoothing width. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot  = in.mdot
out.P     = in.P
Tsat      = T_sat(fluid$, P=in.P)
hf        = Enthalpy(fluid$, P=in.P, x=0)
L_need    = in.mdot * (in.h - hf) / (U_cond * pi# * D * (Tsat - wall.T))
L_cond    = 0.5 * (L_need + L - sqrt((L_need - L)^2 + eps_zone^2))
Q_cond    = U_cond * pi# * D * L_cond * (Tsat - wall.T)
L_sc      = L - L_cond
r_sc      = zone_ramp(L_sc, eps_zone)
T_out     = Temperature(fluid$, P=out.P, h=out.h)
Q_sc      = U_sc * pi# * D * L_sc * (0.5 * (Tsat + T_out) - wall.T) * r_sc
out.h     = in.h - (Q_cond + Q_sc) / in.mdot
Q         = Q_cond + Q_sc
wall.Qdot = -Q
SC        = Tsat - T_out
```
