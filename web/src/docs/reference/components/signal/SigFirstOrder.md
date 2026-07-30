---
name: SigFirstOrder
category: Component (signal)
summary: Acausal signal-domain component SigFirstOrder with ports in, out.
related: []
examples: []
tags: [sigfirstorder, component, signal, acausal]
references: []
generated: true
---

# SigFirstOrder

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigFirstOrder inst(tau, y0)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `tau` | Number |
| `y0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(y)  = (in.sig - y) / tau
init(y) = y0
out.sig = y
```
