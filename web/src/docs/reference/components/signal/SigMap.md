---
name: SigMap
category: Component (signal)
summary: Acausal signal-domain component SigMap with ports in, out.
related: []
examples: []
tags: [sigmap, component, signal, acausal]
references: []
generated: true
---

# SigMap

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigMap inst(map$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `map$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.sig = map$(in.sig)
```
