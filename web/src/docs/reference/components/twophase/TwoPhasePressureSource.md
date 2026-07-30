---
name: TwoPhasePressureSource
category: Component (twophase)
summary: A two-phase boundary fixing the pressure (source).
related: []
examples: [ev-thermal-management]
tags: [twophasepressuresource, component, twophase, acausal]
---

# TwoPhasePressureSource

A two-phase boundary fixing the pressure (source).

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`out`

## Usage

```
TwoPhasePressureSource inst(fluid$, P, x, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `P` | Number | Pressure [Pa]. |
| `x` | Number | Vapor quality / fraction (0–1). |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.P = P
out.h = Enthalpy(fluid$, P=P, x=x)
```

## Examples

Instantiated in the verified example below:

[Run: ev-thermal-management]
