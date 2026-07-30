---
name: TwoPhaseExpansionValve
category: Component (twophase)
summary: A refrigerant expansion valve (isenthalpic throttle).
related: []
examples: []
tags: [twophaseexpansionvalve, component, twophase, acausal]
---

# TwoPhaseExpansionValve

A refrigerant expansion valve (isenthalpic throttle).

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
TwoPhaseExpansionValve inst(fluid$, Cv, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `Cv` | Number | Flow coefficient. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.h    = in.h
rho_in   = Density(fluid$, P=in.P, h=in.h)
in.mdot * abs(in.mdot) = Cv^2 * 2 * rho_in * (in.P - out.P)
```
