---
name: HydroTurbine
category: Component (liquid)
summary: Acausal liquid-domain component HydroTurbine with ports in, out, shaft.
related: []
examples: []
tags: [hydroturbine, component, liquid, acausal]
references: []
generated: true
---

# HydroTurbine

Reusable acausal **liquid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
HydroTurbine inst(fluid$, rho, eta$, epsw, domain$)
```

## Ports

`in`, `out`, `shaft`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `rho` | Number |
| `eta$` | String |
| `epsw` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot  = in.mdot
dP        = in.P - out.P
Pf        = (in.mdot / rho) * dP
eta       = eta$(in.mdot)
Pm        = eta * Pf
shaft.tau = -Pm / (shaft.w + epsw)
out.h     = in.h - Pm / in.mdot
```
