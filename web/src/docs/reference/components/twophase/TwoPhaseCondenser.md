---
name: TwoPhaseCondenser
category: Component (twophase)
summary: A two-phase condenser rejecting heat from the refrigerant.
related: []
examples: []
tags: [twophasecondenser, component, twophase, acausal]
---

# TwoPhaseCondenser

A two-phase condenser rejecting heat from the refrigerant.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
TwoPhaseCondenser inst(fluid$, SC_set, dP, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `SC_set` | Number | Target subcooling [K]. |
| `dP` | Number | Nominal pressure drop [Pa]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.P    = in.P - dP
Tsat     = T_sat(fluid$, P=out.P)
out.h    = Enthalpy(fluid$, P=out.P, T=Tsat - SC_set)
Q        = in.mdot * (in.h - out.h)
```
