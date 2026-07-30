---
name: Infiltration
category: Component (moistair)
summary: Acausal moistair-domain component Infiltration with ports in, out.
related: []
examples: []
tags: [infiltration, component, moistair, acausal]
references: []
generated: true
---

# Infiltration

Reusable acausal **moistair-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
Infiltration inst(C_inf, n_exp, eps, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `C_inf` | Number |
| `n_exp` | Number |
| `eps` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
dP       = in.P - out.P
in.mdot  = C_inf * dP * (dP^2 + eps^2)^((n_exp - 1) / 2)
out.mdot = in.mdot
out.W    = in.W
out.h    = in.h
```
