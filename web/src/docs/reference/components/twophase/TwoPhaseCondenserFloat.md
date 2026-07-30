---
name: TwoPhaseCondenserFloat
category: Component (twophase)
summary: A two-phase condenser whose pressure floats with the charge/ambient balance.
related: []
examples: [ev-thermal-management]
tags: [twophasecondenserfloat, component, twophase, acausal]
---

# TwoPhaseCondenserFloat

A two-phase condenser whose pressure floats with the charge/ambient balance.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
TwoPhaseCondenserFloat inst(fluid$, UA, T_amb, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `UA` | Number | Overall conductance UA [W/K]. |
| `T_amb` | Number | Ambient temperature [K]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.P    = in.P
Tcond    = T_sat(fluid$, P=in.P)
out.h    = Enthalpy(fluid$, P=in.P, x=0)
Q        = in.mdot * (in.h - out.h)
Q        = UA * (Tcond - T_amb)
```

## Examples

Instantiated in the verified example below:

[Run: ev-thermal-management]
