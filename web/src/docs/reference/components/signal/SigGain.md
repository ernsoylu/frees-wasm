---
name: SigGain
category: Component (signal)
summary: Acausal signal-domain component SigGain with ports in, out.
related: []
examples: []
tags: [siggain, component, signal, acausal]
references: []
generated: true
---

# SigGain

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigGain inst(k)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `k` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.sig = k * in.sig
```
