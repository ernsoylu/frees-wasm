---
name: LiquidSource
category: Component (liquid)
summary: A liquid boundary supplying a stream of set state.
related: []
examples: [ev-thermal-management]
tags: [liquidsource, component, liquid, acausal]
---

# LiquidSource

A liquid boundary supplying a stream of set state.

## Domain

A reusable **acausal liquid-domain** component — its single-phase liquid-coolant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`out`

## Usage

```
LiquidSource inst(fluid$, mdot, P, T, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `mdot` | Number | Mass flow rate [kg/s]. |
| `P` | Number | Pressure [Pa]. |
| `T` | Number | Temperature [K]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = mdot
out.P    = P
out.h    = Enthalpy(fluid$, P=P, T=T)
```

## Examples

Instantiated in the verified example below:

[Run: ev-thermal-management]
