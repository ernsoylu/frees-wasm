---
name: LiquidCheckValve
category: Component (liquid)
summary: Acausal liquid-domain component LiquidCheckValve with ports in, out.
related: []
examples: []
tags: [liquidcheckvalve, component, liquid, acausal]
references: []
generated: true
---

# LiquidCheckValve

Reusable acausal **liquid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
LiquidCheckValve inst(CdA, rho, eps, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `CdA` | Number |
| `rho` | Number |
| `eps` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.h    = in.h
dP       = in.P - out.P
fwd      = 0.5 * (1 + tanh(dP / eps))
in.mdot * abs(in.mdot) = fwd * CdA^2 * 2 * rho * dP
```
