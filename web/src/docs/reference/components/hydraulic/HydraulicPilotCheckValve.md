---
name: HydraulicPilotCheckValve
category: Component (hydraulic)
summary: Acausal hydraulic-domain component HydraulicPilotCheckValve with ports in, out, pilot.
related: []
examples: []
tags: [hydraulicpilotcheckvalve, component, hydraulic, acausal]
references: []
generated: true
---

# HydraulicPilotCheckValve

Reusable acausal **hydraulic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
HydraulicPilotCheckValve inst(CdA, rho, rp, eps, domain$)
```

## Ports

`in`, `out`, `pilot`

## Parameters

| Parameter | Type |
| --- | --- |
| `CdA` | Number |
| `rho` | Number |
| `rp` | Number |
| `eps` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
pilot.mdot = 0
out.mdot   = in.mdot
out.h      = in.h
dPe        = (in.P - out.P) + rp * (pilot.P - in.P)
g          = 0.5 * (1 + tanh(dPe / eps))
in.mdot * abs(in.mdot) = g * CdA^2 * 2 * rho * (in.P - out.P)
```
