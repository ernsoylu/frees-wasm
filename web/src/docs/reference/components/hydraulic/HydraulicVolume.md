---
name: HydraulicVolume
category: Component (hydraulic)
summary: Acausal hydraulic-domain component HydraulicVolume with ports in, out.
related: []
examples: []
tags: [hydraulicvolume, component, hydraulic, acausal]
references: []
generated: true
---

# HydraulicVolume

Reusable acausal **hydraulic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
HydraulicVolume inst(V, beta, rho, P0, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `V` | Number |
| `beta` | Number |
| `rho` | Number |
| `P0` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.P      = in.P
out.h      = in.h
der(in.P)  = (beta / (V * rho)) * (in.mdot - out.mdot)
init(in.P) = P0
```
