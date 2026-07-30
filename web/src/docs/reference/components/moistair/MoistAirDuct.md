---
name: MoistAirDuct
category: Component (moistair)
summary: Acausal moistair-domain component MoistAirDuct with ports in, out.
related: []
examples: []
tags: [moistairduct, component, moistair, acausal]
references: []
generated: true
---

# MoistAirDuct

Reusable acausal **moistair-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
MoistAirDuct inst(L, D, rough, mu_a, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `L` | Number |
| `D` | Number |
| `rough` | Number |
| `mu_a` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.W    = in.W
out.h    = in.h
rho      = 1 / Volume(AirH2O, h=in.h, P=in.P, W=in.W)
A        = pi# / 4 * D^2
V        = in.mdot * (1 + in.W) / (rho * A)
Re_d     = reynolds(rho, V, D, mu_a)
f        = friction_factor(Re_d, rough / D)
out.P    = in.P - f * (L / D) * rho * V^2 / 2
```
