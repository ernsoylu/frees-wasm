---
name: SuctionAccumulator
category: Component (twophase)
summary: Acausal twophase-domain component SuctionAccumulator with ports in, out.
related: []
examples: []
tags: [suctionaccumulator, component, twophase, acausal]
references: []
generated: true
---

# SuctionAccumulator

Reusable acausal **twophase-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SuctionAccumulator inst(fluid$, m0, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `m0` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.P   = in.P
hf      = Enthalpy(fluid$, P=in.P, x=0)
hg      = Enthalpy(fluid$, P=in.P, x=1)
out.h   = hg
der(m)  = in.mdot - out.mdot
init(m) = m0
hf * (in.mdot - out.mdot) = in.mdot * in.h - out.mdot * hg
```
