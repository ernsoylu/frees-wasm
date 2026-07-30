---
name: VacuumEjector
category: Component (pneumatic)
summary: Acausal pneumatic-domain component VacuumEjector with ports sup_in, suc_in, exh_out.
related: []
examples: []
tags: [vacuumejector, component, pneumatic, acausal]
references: []
generated: true
---

# VacuumEjector

Reusable acausal **pneumatic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
VacuumEjector inst(fluid$, C, b, ER, domain$)
```

## Ports

`sup_in`, `suc_in`, `exh_out`

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
T_s          = Temperature(fluid$, P=sup_in.P, h=sup_in.h)
sup_in.mdot  = iso6358(C, b, sup_in.P, T_s, exh_out.P)
suc_in.mdot  = ER * sup_in.mdot
exh_out.mdot = sup_in.mdot + suc_in.mdot
exh_out.mdot * exh_out.h = sup_in.mdot * sup_in.h + suc_in.mdot * suc_in.h
```
