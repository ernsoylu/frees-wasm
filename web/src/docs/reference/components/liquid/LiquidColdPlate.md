---
name: LiquidColdPlate
category: Component (liquid)
summary: A liquid cold plate cooling an electronics/heat load.
related: []
examples: []
tags: [liquidcoldplate, component, liquid, acausal]
---

# LiquidColdPlate

A liquid cold plate cooling an electronics/heat load.

## Domain

A reusable **acausal liquid-domain** component — its single-phase liquid-coolant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
LiquidColdPlate inst(Q, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `Q` | Number | Heat input [W]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.P    = in.P
Q        = in.mdot * (out.h - in.h)
```
