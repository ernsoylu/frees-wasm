---
name: TransSpring
category: Component (mechanical)
summary: Acausal mechanical-domain component TransSpring with ports a, b.
related: []
examples: []
tags: [transspring, component, mechanical, acausal]
references: []
generated: true
---

# TransSpring

Reusable acausal **mechanical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
TransSpring inst(k, x0)
```

## Ports

`a`, `b`

## Parameters

| Parameter | Type |
| --- | --- |
| `k` | Number |
| `x0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(x)  = a.vel - b.vel
init(x) = x0
a.f     = k * x
a.f + b.f = 0
```
