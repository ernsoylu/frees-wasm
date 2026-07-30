---
name: SigRelay
category: Component (signal)
summary: Acausal signal-domain component SigRelay with ports in, out.
related: []
examples: []
tags: [sigrelay, component, signal, acausal]
references: []
generated: true
---

# SigRelay

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigRelay inst(thresh, low, high, eps)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `thresh` | Number |
| `low` | Number |
| `high` | Number |
| `eps` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.sig = low + (high - low) * 0.5 * (1 + tanh((in.sig - thresh) / eps))
```
