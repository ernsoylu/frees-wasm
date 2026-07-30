---
name: TwoPhaseEnthalpySource
category: Component (twophase)
summary: A two-phase boundary fixing the stream enthalpy.
related: []
examples: []
tags: [twophaseenthalpysource, component, twophase, acausal]
---

# TwoPhaseEnthalpySource

A two-phase boundary fixing the stream enthalpy.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`out`

## Usage

```
TwoPhaseEnthalpySource inst(mdot, h, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `mdot` | Number | Mass flow rate [kg/s]. |
| `h` | Number | Heat-transfer coefficient [W/m²·K]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = mdot
out.h    = h
```
