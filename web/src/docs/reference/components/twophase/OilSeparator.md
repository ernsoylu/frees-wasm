---
name: OilSeparator
category: Component (twophase)
summary: Acausal twophase-domain component OilSeparator with ports in, out, bleed.
related: []
examples: []
tags: [oilseparator, component, twophase, acausal]
references: []
generated: true
---

# OilSeparator

Reusable acausal **twophase-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
OilSeparator inst(fluid$, f, domain$)
```

## Ports

`in`, `out`, `bleed`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `f` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
bleed.mdot = f * in.mdot
out.mdot   = (1 - f) * in.mdot
out.P      = in.P
bleed.P    = in.P
bleed.h    = Enthalpy(fluid$, P=in.P, x=0)
out.mdot * out.h = in.mdot * in.h - bleed.mdot * bleed.h
```
