---
name: TwoPhasePressureSink
category: Component (twophase)
summary: A two-phase boundary fixing the pressure (sink).
related: []
examples: [pressure-cooker]
tags: [twophasepressuresink, component, twophase, acausal]
---

# TwoPhasePressureSink

A two-phase boundary fixing the pressure (sink).

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`

## Usage

```
TwoPhasePressureSink inst(P, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `P` | Number | Pressure [Pa]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
in.P = P
```

## Examples

Instantiated in the verified example below:

[Run: pressure-cooker]
