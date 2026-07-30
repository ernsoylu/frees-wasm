---
name: Lever
category: Component (mechanical)
summary: Acausal mechanical-domain component Lever with ports a, b.
related: []
examples: []
tags: [lever, component, mechanical, acausal]
references: []
generated: true
---

# Lever

Reusable acausal **mechanical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
Lever inst(ratio)
```

## Ports

`a`, `b`

## Parameters

| Parameter | Type |
| --- | --- |
| `ratio` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
a.vel = ratio * b.vel
b.f   = -ratio * a.f
```
