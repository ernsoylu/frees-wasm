---
name: TwoPhaseOilRider
category: Component (twophase)
summary: Acausal twophase-domain component TwoPhaseOilRider with ports in, out.
related: []
examples: []
tags: [twophaseoilrider, component, twophase, acausal]
references: []
generated: true
---

# TwoPhaseOilRider

Reusable acausal **twophase-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
TwoPhaseOilRider inst(oc_set, k_deg, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `oc_set` | Number |
| `k_deg` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.P    = in.P
out.h    = in.h
out.oc   = oc_set
f_deg    = 1 - k_deg * oc_set
```
