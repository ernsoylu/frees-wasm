---
name: TwoPhaseMixer
category: Component (twophase)
summary: Mixes two two-phase streams with flow-weighted enthalpy.
related: []
examples: [ev-thermal-management]
tags: [twophasemixer, component, twophase, acausal]
---

# TwoPhaseMixer

Mixes two two-phase streams with flow-weighted enthalpy.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in1`, `in2`, `out`

## Usage

```
TwoPhaseMixer inst(domain$)
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
```

## Examples

Instantiated in the verified example below:

[Run: ev-thermal-management]
