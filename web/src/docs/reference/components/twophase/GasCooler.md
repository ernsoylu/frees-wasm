---
name: GasCooler
category: Component (twophase)
summary: Acausal twophase-domain component GasCooler with ports in, out, wall.
related: []
examples: []
tags: [gascooler, component, twophase, acausal]
references: []
generated: true
---

# GasCooler

Reusable acausal **twophase-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
GasCooler inst(fluid$, UA, dP, domain$)
```

## Ports

`in`, `out`, `wall`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `UA` | Number |
| `dP` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot  = in.mdot
out.P     = in.P - dP
T_in      = Temperature(fluid$, P=in.P, h=in.h)
cp_g      = Cp(fluid$, P=in.P, h=in.h)
epsg      = 1 - exp(-UA / (in.mdot * cp_g))
Q         = epsg * in.mdot * cp_g * (T_in - wall.T)
out.h     = in.h - Q / in.mdot
wall.Qdot = -Q
```
