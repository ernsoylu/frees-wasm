---
name: BlendSensor
category: Component (twophase)
summary: A sensor reading the state of a gas-blend stream.
related: []
examples: []
tags: [blendsensor, component, twophase, acausal]
---

# BlendSensor

A sensor reading the state of a gas-blend stream.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
BlendSensor inst(fluid$, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.P    = in.P
out.h    = in.h
out.z    = in.z
hf       = Enthalpy(fluid$, P=in.P, x=0)
hg       = Enthalpy(fluid$, P=in.P, x=1)
x        = (in.h - hf) / (hg - hf)
bubble   = Temperature(fluid$, P=in.P, x=0)
dew      = Temperature(fluid$, P=in.P, x=1)
glide    = dew - bubble
```
