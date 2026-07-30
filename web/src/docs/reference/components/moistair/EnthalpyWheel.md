---
name: EnthalpyWheel
category: Component (moistair)
summary: Acausal moistair-domain component EnthalpyWheel with ports sup_in, sup_out, exh_in, exh_out.
related: []
examples: []
tags: [enthalpywheel, component, moistair, acausal]
references: []
generated: true
---

# EnthalpyWheel

Reusable acausal **moistair-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
EnthalpyWheel inst(eff_h, eff_w, domain$)
```

## Ports

`sup_in`, `sup_out`, `exh_in`, `exh_out`

## Parameters

| Parameter | Type |
| --- | --- |
| `eff_h` | Number |
| `eff_w` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
sup_out.mdot = sup_in.mdot
exh_out.mdot = exh_in.mdot
sup_out.P    = sup_in.P
exh_out.P    = exh_in.P
sup_out.W    = sup_in.W + eff_w * (exh_in.W - sup_in.W)
sup_out.h    = sup_in.h + eff_h * (exh_in.h - sup_in.h)
exh_out.W    = exh_in.W - (sup_in.mdot / exh_in.mdot) * (sup_out.W - sup_in.W)
exh_out.h    = exh_in.h - (sup_in.mdot / exh_in.mdot) * (sup_out.h - sup_in.h)
```
