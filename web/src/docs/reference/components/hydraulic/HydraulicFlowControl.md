---
name: HydraulicFlowControl
category: Component (hydraulic)
summary: Acausal hydraulic-domain component HydraulicFlowControl with ports in, out.
related: []
examples: []
tags: [hydraulicflowcontrol, component, hydraulic, acausal]
references: []
generated: true
---

# HydraulicFlowControl

Reusable acausal **hydraulic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
HydraulicFlowControl inst(Qset, rho, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `Qset` | Number |
| `rho` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.h    = in.h
in.mdot  = rho * Qset
```
