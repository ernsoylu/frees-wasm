---
name: HeatingCoil
category: Component (moistair)
summary: Heats a humid-air stream at constant humidity ratio.
related: []
examples: []
tags: [heatingcoil, component, moistair, acausal]
---

# HeatingCoil

Heats a humid-air stream at constant humidity ratio.

## Domain

A reusable **acausal moistair-domain** component — its humid-air ports carry pressure `P`, dry-air mass-flow `ṁ_da`, enthalpy `h`, and humidity ratio `W`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
HeatingCoil inst(Q, domain$)
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
out.W    = in.W
out.h    = in.h + Q / in.mdot
```
