---
name: MultiZoneWall
category: Component (heat)
summary: Acausal heat-domain component MultiZoneWall with ports a, b.
related: []
examples: []
tags: [multizonewall, component, heat, acausal]
references: []
generated: true
---

# MultiZoneWall

Reusable acausal **heat-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
MultiZoneWall inst(h_a, h_b, U, A, C1, C2, T10, T20)
```

## Ports

`a`, `b`

## Parameters

| Parameter | Type |
| --- | --- |
| `h_a` | Number |
| `h_b` | Number |
| `U` | Number |
| `A` | Number |
| `C1` | Number |
| `C2` | Number |
| `T10` | Number |
| `T20` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
qa       = h_a * A * (a.T - T1)
a.Qdot   = qa
q        = U * A * (T1 - T2)
der(T1)  = (qa - q) / C1
init(T1) = T10
qb       = h_b * A * (T2 - b.T)
b.Qdot   = -qb
der(T2)  = (q - qb) / C2
init(T2) = T20
```
