---
name: CapillaryTube
category: Component (twophase)
summary: Acausal twophase-domain component CapillaryTube with ports in, out.
related: []
examples: []
tags: [capillarytube, component, twophase, acausal]
references: []
generated: true
---

# CapillaryTube

Reusable acausal **twophase-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
CapillaryTube inst(fluid$, C, n, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `C` | Number |
| `n` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.h    = in.h
T_in     = Temperature(fluid$, P=in.P, h=in.h)
Pf       = P_sat(fluid$, T=T_in)
dP_eff   = in.P - max(out.P, Pf)
in.mdot  = C * dP_eff^n
```
