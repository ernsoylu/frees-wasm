---
name: EXVCmd
category: Component (ac)
summary: Acausal ac-domain component EXVCmd with ports in, out, u.
related: []
examples: []
tags: [exvcmd, component, ac, acausal]
references: []
generated: true
---

# EXVCmd

Reusable acausal **ac-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
EXVCmd inst(fluid$, CdA_max, domain$)
```

## Ports

`in`, `out`, `u`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `CdA_max` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.h    = in.h
rho_in   = Density(fluid$, P=in.P, h=in.h)
in.mdot * abs(in.mdot) = (u.sig * CdA_max)^2 * 2 * rho_in * (in.P - out.P)
```
