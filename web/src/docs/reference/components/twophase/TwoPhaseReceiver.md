---
name: TwoPhaseReceiver
category: Component (twophase)
summary: A liquid receiver buffering refrigerant charge at saturation.
related: []
examples: []
tags: [twophasereceiver, component, twophase, acausal]
---

# TwoPhaseReceiver

A liquid receiver buffering refrigerant charge at saturation.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
TwoPhaseReceiver inst(fluid$, V, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `V` | Number | Volume [m³]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.P    = in.P
out.h    = Enthalpy(fluid$, P=in.P, x=0)
rho_l    = Density(fluid$, P=in.P, x=0)
rho_g    = Density(fluid$, P=in.P, x=1)
m        = V * (LL * rho_l + (1 - LL) * rho_g)
```
