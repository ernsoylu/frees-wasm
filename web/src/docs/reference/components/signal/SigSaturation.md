---
name: SigSaturation
category: Component (signal)
summary: Acausal signal-domain component SigSaturation with ports in, out.
related: []
examples: []
tags: [sigsaturation, component, signal, acausal]
references: []
generated: true
---

# SigSaturation

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigSaturation inst(lo, hi)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `lo` | Number |
| `hi` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.sig = min(max(in.sig, lo), hi)
```
