---
name: LiquidThreeWayValve
category: Component (liquid)
summary: Acausal liquid-domain component LiquidThreeWayValve with ports in, outa, outb.
related: []
examples: []
tags: [liquidthreewayvalve, component, liquid, acausal]
references: []
generated: true
---

# LiquidThreeWayValve

Reusable acausal **liquid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
LiquidThreeWayValve inst(u, domain$)
```

## Ports

`in`, `outa`, `outb`

## Parameters

| Parameter | Type |
| --- | --- |
| `u` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
outa.P    = in.P
outb.P    = in.P
outa.h    = in.h
outb.h    = in.h
outa.mdot = u * in.mdot
outb.mdot = (1 - u) * in.mdot
```
