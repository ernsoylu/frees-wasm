---
name: HydraulicFlowDivider
category: Component (hydraulic)
summary: Acausal hydraulic-domain component HydraulicFlowDivider with ports in, outa, outb.
related: []
examples: []
tags: [hydraulicflowdivider, component, hydraulic, acausal]
references: []
generated: true
---

# HydraulicFlowDivider

Reusable acausal **hydraulic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
HydraulicFlowDivider inst(frac, domain$)
```

## Ports

`in`, `outa`, `outb`

## Parameters

| Parameter | Type |
| --- | --- |
| `frac` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
outa.mdot = frac * in.mdot
outb.mdot = (1 - frac) * in.mdot
outa.h    = in.h
outb.h    = in.h
in.P      = frac * outa.P + (1 - frac) * outb.P
```
