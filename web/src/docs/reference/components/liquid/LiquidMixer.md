---
name: LiquidMixer
category: Component (liquid)
summary: Mixes two single-phase liquid streams.
related: []
examples: [ev-thermal-management]
tags: [liquidmixer, component, liquid, acausal]
---

# LiquidMixer

Mixes two single-phase liquid streams.

## Domain

A reusable **acausal liquid-domain** component — its single-phase liquid-coolant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in1`, `in2`, `out`

## Usage

```
LiquidMixer inst(domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.P    = in1.P
in2.P    = in1.P
out.mdot = in1.mdot + in2.mdot
out.mdot * out.h = in1.mdot * in1.h + in2.mdot * in2.h
```

## Examples

Instantiated in the verified example below:

[Run: ev-thermal-management]
