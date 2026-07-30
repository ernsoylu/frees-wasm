---
name: RackPinion
category: Component (mechanical)
summary: Acausal mechanical-domain component RackPinion with ports shaft, rod.
related: []
examples: []
tags: [rackpinion, component, mechanical, acausal]
references: []
generated: true
---

# RackPinion

Reusable acausal **mechanical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
RackPinion inst(r)
```

## Ports

`shaft`, `rod`

## Parameters

| Parameter | Type |
| --- | --- |
| `r` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
rod.vel   = r * shaft.w
shaft.tau = -r * rod.f
```
