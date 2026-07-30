---
name: EndStop
category: Component (mechanical)
summary: Acausal mechanical-domain component EndStop with ports port.
related: []
examples: []
tags: [endstop, component, mechanical, acausal]
references: []
generated: true
---

# EndStop

Reusable acausal **mechanical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
EndStop inst(gap, k, c, eps, x0)
```

## Ports

`port`

## Parameters

| Parameter | Type |
| --- | --- |
| `gap` | Number |
| `k` | Number |
| `c` | Number |
| `eps` | Number |
| `x0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(x)  = port.vel
init(x) = x0
pen     = 0.5 * ((x - gap) + sqrt((x - gap)^2 + eps^2))
port.f  = k * pen + c * port.vel * 0.5 * (1 + tanh((x - gap) / eps))
```
