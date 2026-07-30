---
name: TwoPhasePipe
category: Component (twophase)
summary: A two-phase pipe with a Lockhart–Martinelli frictional pressure drop.
related: []
examples: []
tags: [twophasepipe, component, twophase, acausal]
---

# TwoPhasePipe

A two-phase pipe with a Lockhart–Martinelli frictional pressure drop.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
TwoPhasePipe inst(fluid$, L, D, rough, x, rho_l, rho_g, mu_l, mu_g)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `L` | Number | Length [m]. |
| `D` | Number | Diameter [m]. |
| `rough` | Number | Relative wall roughness. |
| `x` | Number | Vapor quality / fraction (0–1). |
| `rho_l` | Number | Liquid density [kg/m³]. |
| `rho_g` | Number | Vapor density [kg/m³]. |
| `mu_l` | Number | Liquid viscosity [Pa·s]. |
| `mu_g` | Number | Vapor viscosity [Pa·s]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.h    = in.h
A        = pi# / 4 * D^2
V_l      = in.mdot * (1 - x) / (rho_l * A)
Re_l     = reynolds(rho_l, V_l, D, mu_l)
f_l      = friction_factor(Re_l, rough / D)
dP_l     = f_l * (L / D) * rho_l * V_l^2 / 2
X_tt     = lm_martinelli_tt(x, rho_l, rho_g, mu_l, mu_g)
phi2     = lm_phi2(X_tt, 20)
out.P    = in.P - phi2 * dP_l
```
