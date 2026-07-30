---
name: HydraulicValveCmd
category: Component (hydraulic)
summary: Acausal hydraulic-domain component HydraulicValveCmd with ports in, out, u.
related: []
examples: []
tags: [hydraulicvalvecmd, component, hydraulic, acausal]
references: []
generated: true
---

# HydraulicValveCmd

Reusable acausal **hydraulic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
HydraulicValveCmd inst(CdA_max, rho, domain$)
```

## Ports

`in`, `out`, `u`

## Parameters

| Parameter | Type |
| --- | --- |
| `CdA_max` | Number |
| `rho` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.h    = in.h
in.mdot * abs(in.mdot) = (u.sig * CdA_max)^2 * 2 * rho * (in.P - out.P)
```
