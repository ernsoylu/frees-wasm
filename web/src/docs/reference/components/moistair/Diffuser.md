---
name: Diffuser
category: Component (moistair)
summary: Acausal moistair-domain component Diffuser with ports in, out.
related: []
examples: []
tags: [diffuser, component, moistair, acausal]
references: []
generated: true
---

# Diffuser

Reusable acausal **moistair-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
Diffuser inst(A1, A2, eta_rec, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `A1` | Number |
| `A2` | Number |
| `eta_rec` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.W    = in.W
out.h    = in.h
rho      = 1 / Volume(AirH2O, h=in.h, P=in.P, W=in.W)
V1       = in.mdot * (1 + in.W) / (rho * A1)
out.P    = in.P + eta_rec * 0.5 * rho * V1^2 * (1 - (A1 / A2)^2)
```
