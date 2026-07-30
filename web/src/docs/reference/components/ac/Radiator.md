---
name: Radiator
category: Component (ac)
summary: Acausal ac-domain component Radiator with ports cool_in, cool_out, air_in, air_out.
related: []
examples: []
tags: [radiator, component, ac, acausal]
references: []
generated: true
---

# Radiator

Reusable acausal **ac-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
Radiator inst(cool$, UA_cool, eps_air)
```

## Ports

`cool_in`, `cool_out`, `air_in`, `air_out`

## Parameters

| Parameter | Type |
| --- | --- |
| `cool$` | String |
| `UA_cool` | Number |
| `eps_air` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
LiquidWallHX   CL(fluid$=cool$, UA=UA_cool)
MoistAirWallHX AR(model$=eps_t, eps=eps_air)
connect(cool_in, CL.in)
connect(CL.out, cool_out)
connect(air_in, AR.in)
connect(AR.out, air_out)
connect(CL.wall, AR.wall)
```
