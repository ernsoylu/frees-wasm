---
name: Brake
category: Component (mechanical)
summary: Acausal mechanical-domain component Brake with ports a, b, u.
related: []
examples: []
tags: [brake, component, mechanical, acausal]
references: []
generated: true
---

# Brake

Reusable acausal **mechanical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
Brake inst(Tmax, eps)
```

## Ports

`a`, `b`, `u`

## Parameters

| Parameter | Type |
| --- | --- |
| `Tmax` | Number |
| `eps` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
dw    = a.w - b.w
a.tau = u.sig * Tmax * tanh(dw / eps)
a.tau + b.tau = 0
```
