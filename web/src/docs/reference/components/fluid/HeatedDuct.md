---
name: HeatedDuct
category: Component (fluid)
summary: Acausal fluid-domain component HeatedDuct with ports in, out, wall.
related: []
examples: []
tags: [heatedduct, component, fluid, acausal]
references: []
generated: true
---

# HeatedDuct

Reusable acausal **fluid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
HeatedDuct inst(fluid$, UA)
```

## Ports

`in`, `out`, `wall`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `UA` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot  = in.mdot
out.P     = in.P
T_in      = Temperature(fluid$, P=in.P, h=in.h)
cp_d      = Cp(fluid$, P=in.P, h=in.h)
epsd      = 1 - exp(-UA / (in.mdot * cp_d))
Q         = epsd * in.mdot * cp_d * (wall.T - T_in)
out.h     = in.h + Q / in.mdot
wall.Qdot = Q
```
