---
name: Cam
category: Component (mechanical)
summary: Acausal mechanical-domain component Cam with ports shaft, rod.
related: []
examples: []
tags: [cam, component, mechanical, acausal]
references: []
generated: true
---

# Cam

Reusable acausal **mechanical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
Cam inst(prof$, theta0)
```

## Ports

`shaft`, `rod`

## Parameters

| Parameter | Type |
| --- | --- |
| `prof$` | String |
| `theta0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(theta)  = shaft.w
init(theta) = theta0
slope     = dtable(prof$, theta)
lift      = prof$(theta)
rod.vel   = slope * shaft.w
shaft.tau = slope * rod.f
```
