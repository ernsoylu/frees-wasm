---
name: LiquidTank
category: Component (liquid)
summary: Acausal liquid-domain component LiquidTank with ports in, out, wall.
related: []
examples: []
tags: [liquidtank, component, liquid, acausal]
references: []
generated: true
---

# LiquidTank

Reusable acausal **liquid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
LiquidTank inst(fluid$, m, UA, T0, domain$)
```

## Ports

`in`, `out`, `wall`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `m` | Number |
| `UA` | Number |
| `T0` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.P     = in.P
out.mdot  = in.mdot
out.h     = Enthalpy(fluid$, P=in.P, T=Tt)
cp_t      = Cp(fluid$, P=in.P, T=Tt)
Q         = UA * (wall.T - Tt)
der(Tt)   = (in.mdot * (in.h - out.h) + Q) / (m * cp_t)
init(Tt)  = T0
wall.Qdot = Q
```
