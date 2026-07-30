---
name: ClutchCmd
category: Component (mechanical)
summary: Acausal mechanical-domain component ClutchCmd with ports a, b, u.
related: []
examples: []
tags: [clutchcmd, component, mechanical, acausal]
references: []
generated: true
---

# ClutchCmd

Reusable acausal **mechanical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
ClutchCmd inst(Tmax, eps)
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
