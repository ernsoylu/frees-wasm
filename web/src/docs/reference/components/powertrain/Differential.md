---
name: Differential
category: Component (powertrain)
summary: Acausal powertrain-domain component Differential with ports in, left, right.
related: []
examples: []
tags: [differential, component, powertrain, acausal]
references: []
generated: true
---

# Differential

Reusable acausal **powertrain-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
Differential inst(ratio)
```

## Ports

`in`, `left`, `right`

## Parameters

| Parameter | Type |
| --- | --- |
| `ratio` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
in.w      = ratio * 0.5 * (left.w + right.w)
left.tau  = -0.5 * ratio * in.tau
right.tau = -0.5 * ratio * in.tau
```
