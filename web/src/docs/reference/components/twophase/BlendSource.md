---
name: BlendSource
category: Component (twophase)
summary: A boundary supplying a gas-blend stream of set composition.
related: []
examples: []
tags: [blendsource, component, twophase, acausal]
---

# BlendSource

A boundary supplying a gas-blend stream of set composition.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`out`

## Usage

```
BlendSource inst(fluid$, mdot, P, x, z, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `mdot` | Number | Mass flow rate [kg/s]. |
| `P` | Number | Pressure [Pa]. |
| `x` | Number | Vapor quality / fraction (0–1). |
| `z` | Number | Elevation [m]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = mdot
out.P    = P
out.h    = Enthalpy(fluid$, P=P, x=x)
out.z    = z
bubble   = Temperature(fluid$, P=P, x=0)
dew      = Temperature(fluid$, P=P, x=1)
glide    = dew - bubble
```
