---
name: Propeller
category: Component (fluid)
summary: Acausal fluid-domain component Propeller with ports shaft, veh.
related: []
examples: []
tags: [propeller, component, fluid, acausal]
references: []
generated: true
---

# Propeller

Reusable acausal **fluid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
Propeller inst(Dp, rhoA, ct$, cpw$, epsn)
```

## Ports

`shaft`, `veh`

## Parameters

| Parameter | Type |
| --- | --- |
| `Dp` | Number |
| `rhoA` | Number |
| `ct$` | String |
| `cpw$` | String |
| `epsn` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
n         = shaft.w / (2 * pi#)
J         = veh.vel / (n * Dp + epsn)
veh.f     = -(ct$(J) * rhoA * n^2 * Dp^4)
shaft.tau = cpw$(J) * rhoA * n^2 * Dp^5 / (2 * pi#)
```
