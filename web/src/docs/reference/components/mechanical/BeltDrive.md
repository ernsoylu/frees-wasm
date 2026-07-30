---
name: BeltDrive
category: Component (mechanical)
summary: Acausal mechanical-domain component BeltDrive with ports a, b.
related: []
examples: []
tags: [beltdrive, component, mechanical, acausal]
references: []
generated: true
---

# BeltDrive

Reusable acausal **mechanical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
BeltDrive inst(ratio, eta)
```

## Ports

`a`, `b`

## Parameters

| Parameter | Type |
| --- | --- |
| `ratio` | Number |
| `eta` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
a.w   = ratio * b.w
b.tau = -ratio * eta * a.tau
```
