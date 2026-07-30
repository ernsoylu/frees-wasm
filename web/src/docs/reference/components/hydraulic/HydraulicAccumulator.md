---
name: HydraulicAccumulator
category: Component (hydraulic)
summary: Acausal hydraulic-domain component HydraulicAccumulator with ports port.
related: []
examples: []
tags: [hydraulicaccumulator, component, hydraulic, acausal]
references: []
generated: true
---

# HydraulicAccumulator

Reusable acausal **hydraulic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
HydraulicAccumulator inst(P0, V0, gamma, rho, Vg0, domain$)
```

## Ports

`port`

## Parameters

| Parameter | Type |
| --- | --- |
| `P0` | Number |
| `V0` | Number |
| `gamma` | Number |
| `rho` | Number |
| `Vg0` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(Vg)  = -port.mdot / rho
init(Vg) = Vg0
port.P   = P0 * (V0 / Vg)^gamma
```
