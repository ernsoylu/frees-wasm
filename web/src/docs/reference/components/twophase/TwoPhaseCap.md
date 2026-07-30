---
name: TwoPhaseCap
category: Component (twophase)
summary: A two-phase capacitive volume (a pressure-compliance node).
related: []
examples: []
tags: [twophasecap, component, twophase, acausal]
---

# TwoPhaseCap

A two-phase capacitive volume (a pressure-compliance node).

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`

## Usage

```
TwoPhaseCap inst(domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
in.mdot = 0
```
