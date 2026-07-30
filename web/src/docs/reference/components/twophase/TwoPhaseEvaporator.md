---
name: TwoPhaseEvaporator
category: Component (twophase)
summary: A two-phase evaporator absorbing heat into the refrigerant.
related: []
examples: []
tags: [twophaseevaporator, component, twophase, acausal]
---

# TwoPhaseEvaporator

A two-phase evaporator absorbing heat into the refrigerant.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
TwoPhaseEvaporator inst(fluid$, SH_set, dP, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `SH_set` | Number | Target superheat [K]. |
| `dP` | Number | Nominal pressure drop [Pa]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.P    = in.P - dP
Tsat     = T_sat(fluid$, P=out.P)
out.h    = Enthalpy(fluid$, P=out.P, T=Tsat + SH_set)
Q        = in.mdot * (out.h - in.h)
```
