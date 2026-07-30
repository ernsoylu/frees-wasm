---
name: Freewheel
category: Component (mechanical)
summary: Acausal mechanical-domain component Freewheel with ports a, b.
related: []
examples: []
tags: [freewheel, component, mechanical, acausal]
references: []
generated: true
---

# Freewheel

Reusable acausal **mechanical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
Freewheel inst(k, eps)
```

## Ports

`a`, `b`

## Parameters

| Parameter | Type |
| --- | --- |
| `k` | Number |
| `eps` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
dw    = a.w - b.w
a.tau = k * 0.5 * (dw + sqrt(dw^2 + eps^2))
a.tau + b.tau = 0
```
