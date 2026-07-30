---
name: SigTable
category: Component (signal)
summary: Acausal signal-domain component SigTable with ports out.
related: []
examples: []
tags: [sigtable, component, signal, acausal]
references: []
generated: true
---

# SigTable

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigTable inst(map$)
```

## Ports

`out`

## Parameters

| Parameter | Type |
| --- | --- |
| `map$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.sig = map$(time)
```
