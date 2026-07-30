---
name: SigPulse
category: Component (signal)
summary: Acausal signal-domain component SigPulse with ports out.
related: []
examples: []
tags: [sigpulse, component, signal, acausal]
references: []
generated: true
---

# SigPulse

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigPulse inst(t0, width, high, low, eps)
```

## Ports

`out`

## Parameters

| Parameter | Type |
| --- | --- |
| `t0` | Number |
| `width` | Number |
| `high` | Number |
| `low` | Number |
| `eps` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.sig = low + (high - low) * 0.5 * (tanh((time - t0) / eps) - tanh((time - t0 - width) / eps))
```
