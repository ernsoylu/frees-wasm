---
name: TwoPhaseFlowRes
category: Component (twophase)
summary: A two-phase flow resistance relating pressure drop to mass flow.
related: []
examples: []
tags: [twophaseflowres, component, twophase, acausal]
---

# TwoPhaseFlowRes

A two-phase flow resistance relating pressure drop to mass flow.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
TwoPhaseFlowRes inst(fluid$, L, D, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `L` | Number | Length [m]. |
| `D` | Number | Diameter [m]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.h    = in.h
hf       = Enthalpy(fluid$, P=in.P, x=0)
hg       = Enthalpy(fluid$, P=in.P, x=1)
x        = (in.h - hf) / (hg - hf)
rho_l    = Density(fluid$, P=in.P, x=0)
rho_g    = Density(fluid$, P=in.P, x=1)
mu_l     = Viscosity(fluid$, P=in.P, x=0)
mu_g     = Viscosity(fluid$, P=in.P, x=1)
sigma    = SurfaceTension(fluid$, P=in.P)
A        = pi# / 4 * D^2
G        = in.mdot / A
V_lo     = G / rho_l
Re_lo    = reynolds(rho_l, V_lo, D, mu_l)
f_lo     = friction_factor(Re_lo, 0)
dP_lo    = f_lo * (L / D) * rho_l * V_lo^2 / 2
phi2     = friedel_phi2(x, rho_l, rho_g, mu_l, mu_g, G, D, sigma)
out.P    = in.P - phi2 * dP_lo
```
