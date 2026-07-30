---
name: ScrewDrive
category: Component (mechanical)
summary: Acausal mechanical-domain component ScrewDrive with ports shaft, rod.
related: []
examples: []
tags: [screwdrive, component, mechanical, acausal]
references: []
generated: true
---

# ScrewDrive

Reusable acausal **mechanical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
ScrewDrive inst(lead)
```

## Ports

`shaft`, `rod`

## Parameters

| Parameter | Type |
| --- | --- |
| `lead` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
rod.vel   = lead / (2 * pi#) * shaft.w
shaft.tau = -(lead / (2 * pi#)) * rod.f
```
