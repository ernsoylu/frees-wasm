---
name: RadiationTwoSurface
category: Component (heat)
summary: Acausal heat-domain component RadiationTwoSurface with ports a, b.
related: []
examples: []
tags: [radiationtwosurface, component, heat, acausal]
references: []
generated: true
---

# RadiationTwoSurface

Reusable acausal **heat-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
RadiationTwoSurface inst(e1, e2, A1, A2, F12)
```

## Ports

`a`, `b`

## Parameters

| Parameter | Type |
| --- | --- |
| `e1` | Number |
| `e2` | Number |
| `A1` | Number |
| `A2` | Number |
| `F12` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
Rrad   = (1 - e1) / (e1 * A1) + 1 / (A1 * F12) + (1 - e2) / (e2 * A2)
a.Qdot = 5.670374419e-8 * (a.T^4 - b.T^4) / Rrad
a.Qdot + b.Qdot = 0
```
