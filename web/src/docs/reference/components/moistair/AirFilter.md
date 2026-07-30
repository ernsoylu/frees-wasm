---
name: AirFilter
category: Component (moistair)
summary: Acausal moistair-domain component AirFilter with ports in, out.
related: []
examples: []
tags: [airfilter, component, moistair, acausal]
references: []
generated: true
---

# AirFilter

Reusable acausal **moistair-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
AirFilter inst(K, foul, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `K` | Number |
| `foul` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.W    = in.W
out.h    = in.h
out.P    = in.P - foul * K * in.mdot^2
```
