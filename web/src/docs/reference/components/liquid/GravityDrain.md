---
name: GravityDrain
category: Component (liquid)
summary: Acausal liquid-domain component GravityDrain with ports in, out.
related: []
examples: []
tags: [gravitydrain, component, liquid, acausal]
references: []
generated: true
---

# GravityDrain

Reusable acausal **liquid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
GravityDrain inst(Cd, A_d, rho, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `Cd` | Number |
| `A_d` | Number |
| `rho` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
dP = in.P - out.P
in.mdot * abs(in.mdot) = (Cd * A_d)^2 * 2 * rho * dP
out.mdot = in.mdot
out.h    = in.h
```
