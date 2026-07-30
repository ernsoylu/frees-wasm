---
name: SigSine
category: Component (signal)
summary: Acausal signal-domain component SigSine with ports out.
related: []
examples: []
tags: [sigsine, component, signal, acausal]
references: []
generated: true
---

# SigSine

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigSine inst(amp, freq, phase, bias)
```

## Ports

`out`

## Parameters

| Parameter | Type |
| --- | --- |
| `amp` | Number |
| `freq` | Number |
| `phase` | Number |
| `bias` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.sig = bias + amp * sin(2 * pi# * freq * time + phase)
```
