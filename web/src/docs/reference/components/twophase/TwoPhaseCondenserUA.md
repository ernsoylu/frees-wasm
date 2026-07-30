---
name: TwoPhaseCondenserUA
category: Component (twophase)
summary: A two-phase condenser sized by an overall conductance UA.
related: []
examples: []
tags: [twophasecondenserua, component, twophase, acausal]
---

# TwoPhaseCondenserUA

A two-phase condenser sized by an overall conductance `UA`.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
TwoPhaseCondenserUA inst(fluid$, UA, T_amb, V, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `UA` | Number | Overall conductance UA [W/K]. |
| `T_amb` | Number | Ambient temperature [K]. |
| `V` | Number | Volume [m³]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.P    = in.P
Tcond    = T_sat(fluid$, P=in.P)
Q        = UA * (Tcond - T_amb)
Q        = in.mdot * (in.h - out.h)
rho_in   = Density(fluid$, P=in.P, h=in.h)
rho_out  = Density(fluid$, P=out.P, h=out.h)
m        = V * 0.5 * (rho_in + rho_out)
SC       = Tcond - Temperature(fluid$, P=out.P, h=out.h)
```
