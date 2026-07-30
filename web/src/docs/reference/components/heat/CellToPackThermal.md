---
name: CellToPackThermal
category: Component (heat)
summary: Acausal heat-domain component CellToPackThermal with ports cell, plate.
related: []
examples: []
tags: [celltopackthermal, component, heat, acausal]
references: []
generated: true
---

# CellToPackThermal

Reusable acausal **heat-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
CellToPackThermal inst(Rcc, Cpl, T0)
```

## Ports

`cell`, `plate`

## Parameters

| Parameter | Type |
| --- | --- |
| `Rcc` | Number |
| `Cpl` | Number |
| `T0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
Q         = (cell.T - Tp) / Rcc
cell.Qdot = Q
der(Tp)   = (Q + plate.Qdot) / Cpl
init(Tp)  = T0
plate.T   = Tp
```
