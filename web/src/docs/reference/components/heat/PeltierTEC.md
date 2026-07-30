---
name: PeltierTEC
category: Component (heat)
summary: Acausal heat-domain component PeltierTEC with ports p, n, hot, cold.
related: []
examples: []
tags: [peltiertec, component, heat, acausal]
references: []
generated: true
---

# PeltierTEC

Reusable acausal **heat-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
PeltierTEC inst(Sab, Rel, Kth)
```

## Ports

`p`, `n`, `hot`, `cold`

## Parameters

| Parameter | Type |
| --- | --- |
| `Sab` | Number |
| `Rel` | Number |
| `Kth` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
I         = p.I
p.V - n.V = Sab * (hot.T - cold.T) + Rel * I
p.I + n.I = 0
Qc        = Sab * cold.T * I - 0.5 * Rel * I^2 - Kth * (hot.T - cold.T)
Qh        = Sab * hot.T * I + 0.5 * Rel * I^2 - Kth * (hot.T - cold.T)
cold.Qdot = Qc
hot.Qdot  = -Qh
```
