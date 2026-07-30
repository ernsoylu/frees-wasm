---
name: BlendSink
category: Component (twophase)
summary: A boundary absorbing a gas-blend stream.
related: []
examples: []
tags: [blendsink, component, twophase, acausal]
---

# BlendSink

A boundary absorbing a gas-blend stream.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`

## Usage

```
BlendSink inst(domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
mdot  = in.mdot
P     = in.P
h     = in.h
z     = in.z
```
