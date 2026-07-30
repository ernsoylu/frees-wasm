---
name: BlendMixer
category: Component (twophase)
summary: A gas-blend (mixture) mixing junction carrying the species rider.
related: []
examples: []
tags: [blendmixer, component, twophase, acausal]
---

# BlendMixer

A gas-blend (mixture) mixing junction carrying the species rider.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in1`, `in2`, `out`

## Usage

```
BlendMixer inst(domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.P    = in1.P
out.mdot = in1.mdot + in2.mdot
out.mdot * out.h = in1.mdot * in1.h + in2.mdot * in2.h
out.mdot * out.z = in1.mdot * in1.z + in2.mdot * in2.z
```
