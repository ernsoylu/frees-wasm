---
name: HeatPipe
category: Component (heat)
summary: Acausal heat-domain component HeatPipe with ports a, b.
related: []
examples: []
tags: [heatpipe, component, heat, acausal]
references: []
generated: true
---

# HeatPipe

Reusable acausal **heat-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
HeatPipe inst(G, Qmax)
```

## Ports

`a`, `b`

## Parameters

| Parameter | Type |
| --- | --- |
| `G` | Number |
| `Qmax` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
a.Qdot = Qmax * tanh(G * (a.T - b.T) / Qmax)
a.Qdot + b.Qdot = 0
```
