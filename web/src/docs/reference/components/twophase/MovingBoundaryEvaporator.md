---
name: MovingBoundaryEvaporator
category: Component (twophase)
summary: A moving-boundary evaporator tracking the two-phase/superheat zone lengths.
related: []
examples: []
tags: [movingboundaryevaporator, component, twophase, acausal]
---

# MovingBoundaryEvaporator

A moving-boundary evaporator tracking the two-phase/superheat zone lengths.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`, `wall`

## Usage

```
MovingBoundaryEvaporator inst(fluid$, U_tp, U_sh, D, L, eps_zone, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `U_tp` | Number | Two-phase-zone overall coefficient [W/m²·K]. |
| `U_sh` | Number | Superheat-zone overall coefficient [W/m²·K]. |
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
hg        = Enthalpy(fluid$, P=in.P, x=1)
L_need    = in.mdot * (hg - in.h) / (U_tp * pi# * D * (wall.T - Tsat))
L_tp      = 0.5 * (L_need + L - sqrt((L_need - L)^2 + eps_zone^2))
Q_tp      = U_tp * pi# * D * L_tp * (wall.T - Tsat)
L_sh      = L - L_tp
r_sh      = zone_ramp(L_sh, eps_zone)
T_out     = Temperature(fluid$, P=out.P, h=out.h)
Q_sh      = U_sh * pi# * D * L_sh * (wall.T - 0.5 * (Tsat + T_out)) * r_sh
out.h     = in.h + (Q_tp + Q_sh) / in.mdot
Q         = Q_tp + Q_sh
wall.Qdot = Q
SH        = T_out - Tsat
```
