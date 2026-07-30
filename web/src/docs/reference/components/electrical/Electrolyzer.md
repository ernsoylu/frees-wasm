---
name: Electrolyzer
category: Component (electrical)
summary: Acausal electrical-domain component Electrolyzer with ports p, n, heat.
related: []
examples: []
tags: [electrolyzer, component, electrical, acausal]
references: []
generated: true
---

# Electrolyzer

Reusable acausal **electrical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
Electrolyzer inst(ncells, area, i0, Rohm, E0, alpha, Eth, T)
```

## Ports

`p`, `n`, `heat`

## Parameters

| Parameter | Type |
| --- | --- |
| `ncells` | Number |
| `area` | Number |
| `i0` | Number |
| `Rohm` | Number |
| `E0` | Number |
| `alpha` | Number |
| `Eth` | Number |
| `T` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
I_cell    = p.I
i         = I_cell / area
V_cell    = E0 + (8.314 * T / (alpha * 96485)) * ln(i / i0 + 1) + i * Rohm
p.V - n.V = ncells * V_cell
p.I + n.I = 0
mdot_h2   = ncells * I_cell * 2.016e-3 / (2 * 96485)
Q         = I_cell * ncells * (V_cell - Eth)
heat.Qdot = -Q
```
