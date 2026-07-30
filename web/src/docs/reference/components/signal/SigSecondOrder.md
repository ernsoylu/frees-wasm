---
name: SigSecondOrder
category: Component (signal)
summary: Acausal signal-domain component SigSecondOrder with ports in, out.
related: []
examples: []
tags: [sigsecondorder, component, signal, acausal]
references: []
generated: true
---

# SigSecondOrder

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigSecondOrder inst(wn, zeta, y0, v0)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `wn` | Number |
| `zeta` | Number |
| `y0` | Number |
| `v0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(y)  = v
init(y) = y0
der(v)  = wn^2 * (in.sig - y) - 2 * zeta * wn * v
init(v) = v0
out.sig = y
```
