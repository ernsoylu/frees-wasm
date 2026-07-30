---
name: LiquidExpansionTank
category: Component (liquid)
summary: Acausal liquid-domain component LiquidExpansionTank with ports port.
related: []
examples: []
tags: [liquidexpansiontank, component, liquid, acausal]
references: []
generated: true
---

# LiquidExpansionTank

Reusable acausal **liquid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
LiquidExpansionTank inst(P, domain$)
```

## Ports

`port`

## Parameters

| Parameter | Type |
| --- | --- |
| `P` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
port.P = P
```
