---
name: SigRamp
category: Component (signal)
summary: Acausal signal-domain component SigRamp with ports out.
related: []
examples: []
tags: [sigramp, component, signal, acausal]
references: []
generated: true
---

# SigRamp

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigRamp inst(t0, slope, eps)
```

## Ports

`out`

## Parameters

| Parameter | Type |
| --- | --- |
| `t0` | Number |
| `slope` | Number |
| `eps` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
dt      = time - t0
out.sig = slope * 0.5 * (dt + sqrt(dt^2 + eps^2))
```
