---
name: HydraulicPipe
category: Component (hydraulic)
summary: Acausal hydraulic-domain component HydraulicPipe with ports in, out.
related: []
examples: []
tags: [hydraulicpipe, component, hydraulic, acausal]
references: []
generated: true
---

# HydraulicPipe

Reusable acausal **hydraulic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
HydraulicPipe inst(rho, nu, L, D, rough, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `rho` | Number |
| `nu` | Number |
| `L` | Number |
| `D` | Number |
| `rough` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.h    = in.h
A        = pi# / 4 * D^2
V        = in.mdot / (rho * A)
Re       = reynolds(rho, abs(V) + 1e-9, D, rho * nu)
f        = friction_factor(Re, rough / D)
out.P    = in.P - f * (L / D) * rho * V * abs(V) / 2
```
