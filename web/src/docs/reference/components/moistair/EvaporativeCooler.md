---
name: EvaporativeCooler
category: Component (moistair)
summary: Acausal moistair-domain component EvaporativeCooler with ports in, out.
related: []
examples: []
tags: [evaporativecooler, component, moistair, acausal]
references: []
generated: true
---

# EvaporativeCooler

Reusable acausal **moistair-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
EvaporativeCooler inst(eff, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `eff` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.P    = in.P
out.h    = in.h
W_sat    = HumRat(AirH2O, h=in.h, P=in.P, R=1)
out.W    = in.W + eff * (W_sat - in.W)
```
