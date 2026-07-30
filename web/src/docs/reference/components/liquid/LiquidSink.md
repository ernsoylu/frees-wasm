---
name: LiquidSink
category: Component (liquid)
summary: A liquid boundary absorbing a stream.
related: []
examples: [ev-thermal-management]
tags: [liquidsink, component, liquid, acausal]
---

# LiquidSink

A liquid boundary absorbing a stream.

## Domain

A reusable **acausal liquid-domain** component — its single-phase liquid-coolant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`

## Usage

```
LiquidSink inst(domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
mdot = in.mdot
P    = in.P
h    = in.h
```

## Examples

Instantiated in the verified example below:

[Run: ev-thermal-management]
