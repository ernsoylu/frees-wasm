---
name: LiquidThermostat
category: Component (liquid)
summary: Acausal liquid-domain component LiquidThermostat with ports in, out.
related: []
examples: []
tags: [liquidthermostat, component, liquid, acausal]
references: []
generated: true
---

# LiquidThermostat

Reusable acausal **liquid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
LiquidThermostat inst(fluid$, CdA, rho, Topen, Tband, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `CdA` | Number |
| `rho` | Number |
| `Topen` | Number |
| `Tband` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.h    = in.h
T_in     = Temperature(fluid$, P=in.P, h=in.h)
u        = 0.5 * (1 + tanh((T_in - Topen) / Tband))
in.mdot * abs(in.mdot) = (u * CdA)^2 * 2 * rho * (in.P - out.P)
```
