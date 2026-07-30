---
name: WallRC
category: Component (heat)
summary: Acausal heat-domain component WallRC with ports a, b.
related: []
examples: []
tags: [wallrc, component, heat, acausal]
references: []
generated: true
---

# WallRC

Reusable acausal **heat-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
WallRC inst(C1, C2, R, T10, T20)
```

## Ports

`a`, `b`

## Parameters

| Parameter | Type |
| --- | --- |
| `C1` | Number |
| `C2` | Number |
| `R` | Number |
| `T10` | Number |
| `T20` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(T1)  = (a.Qdot - (T1 - T2) / R) / C1
init(T1) = T10
der(T2)  = ((T1 - T2) / R + b.Qdot) / C2
init(T2) = T20
a.T = T1
b.T = T2
```
