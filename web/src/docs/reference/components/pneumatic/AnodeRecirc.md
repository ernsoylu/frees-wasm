---
name: AnodeRecirc
category: Component (pneumatic)
summary: Acausal pneumatic-domain component AnodeRecirc with ports sup_in, ret_in, out.
related: []
examples: []
tags: [anoderecirc, component, pneumatic, acausal]
references: []
generated: true
---

# AnodeRecirc

Reusable acausal **pneumatic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
AnodeRecirc inst(fluid$, C, b, ER, domain$)
```

## Ports

`sup_in`, `ret_in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `C` | Number |
| `b` | Number |
| `ER` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
T_s         = Temperature(fluid$, P=sup_in.P, h=sup_in.h)
sup_in.mdot = iso6358(C, b, sup_in.P, T_s, out.P)
ret_in.mdot = ER * sup_in.mdot
out.mdot    = sup_in.mdot + ret_in.mdot
out.mdot * out.h = sup_in.mdot * sup_in.h + ret_in.mdot * ret_in.h
```
