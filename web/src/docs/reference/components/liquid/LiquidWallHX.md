---
name: LiquidWallHX
category: Component (liquid)
summary: A liquid-to-wall heat exchanger.
related: []
examples: [ev-thermal-management]
tags: [liquidwallhx, component, liquid, acausal]
---

# LiquidWallHX

A liquid-to-wall heat exchanger.

## Domain

A reusable **acausal liquid-domain** component — its single-phase liquid-coolant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`, `wall`

## Usage

```
LiquidWallHX inst(fluid$, UA, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `UA` | Number | Overall conductance UA [W/K]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot  = in.mdot
out.P     = in.P
T_in      = Temperature(fluid$, P=in.P, h=in.h)
Q         = UA * (T_in - wall.T)
out.h     = in.h - Q / in.mdot
wall.Qdot = -Q
```

## Examples

Instantiated in the verified example below:

[Run: ev-thermal-management]
