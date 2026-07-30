---
name: HydraulicResistance
category: Component (hydraulic)
summary: Acausal hydraulic-domain component HydraulicResistance with ports in, out.
related: []
examples: []
tags: [hydraulicresistance, component, hydraulic, acausal]
references: []
generated: true
---

# HydraulicResistance

Reusable acausal **hydraulic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
HydraulicResistance inst(K, rho, D, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `K` | Number |
| `rho` | Number |
| `D` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.h    = in.h
A        = pi# / 4 * D^2
V        = in.mdot / (rho * A)
out.P    = in.P - K * rho * V * abs(V) / 2
```
