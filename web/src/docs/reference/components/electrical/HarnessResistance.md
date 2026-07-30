---
name: HarnessResistance
category: Component (electrical)
summary: Acausal electrical-domain component HarnessResistance with ports a, b, heat.
related: []
examples: []
tags: [harnessresistance, component, electrical, acausal]
references: []
generated: true
---

# HarnessResistance

Reusable acausal **electrical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
HarnessResistance inst(R20, alphaT)
```

## Ports

`a`, `b`, `heat`

## Parameters

| Parameter | Type |
| --- | --- |
| `R20` | Number |
| `alphaT` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
R         = R20 * (1 + alphaT * (heat.T - 293.15))
a.V - b.V = R * a.I
a.I + b.I = 0
Q         = R * a.I^2
heat.Qdot = -Q
```
