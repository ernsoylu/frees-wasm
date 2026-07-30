---
name: TwoPhaseSourcePH
category: Component (twophase)
summary: A two-phase source specified by pressure and enthalpy (P, h).
related: []
examples: []
tags: [twophasesourceph, component, twophase, acausal]
---

# TwoPhaseSourcePH

A two-phase source specified by pressure and enthalpy `(P, h)`.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`out`

## Usage

```
TwoPhaseSourcePH inst(mdot, P, h, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `mdot` | Number | Mass flow rate [kg/s]. |
| `P` | Number | Pressure [Pa]. |
| `h` | Number | Heat-transfer coefficient [W/m²·K]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = mdot
out.P    = P
out.h    = h
```
