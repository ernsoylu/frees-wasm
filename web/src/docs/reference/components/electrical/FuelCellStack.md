---
name: FuelCellStack
category: Component (electrical)
summary: A PEM fuel-cell stack producing voltage from its polarization curve.
related: []
examples: []
tags: [fuelcellstack, component, electrical, acausal]
---

# FuelCellStack

A PEM fuel-cell stack producing voltage from its polarization curve.

## Domain

A reusable **acausal electrical-domain** component — its electrical ports carry potential `V` and current `I`; a node enforces equal `V` and `ΣI = 0` (Kirchhoff). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`p`, `n`, `heat`

## Usage

```
FuelCellStack inst(ncells, area, i0, ilim, Rohm, E0, alpha, Eth, T)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `ncells` | Number | Number of cells. |
| `area` | Number | Area [m²]. |
| `i0` | Number | Initial current [A]. |
| `ilim` | Number | Current limit [A]. |
| `Rohm` | Number | Ohmic resistance [Ω]. |
| `E0` | Number | Reference EMF [V]. |
| `alpha` | Number | Void fraction / coefficient. |
| `Eth` | Number | Activation/threshold energy. |
| `T` | Number | Temperature [K]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
I_cell    = -p.I
i         = I_cell / area
V_cell    = E0 - (8.314 * T / (alpha * 96485)) * ln(i / i0) - i * Rohm - (8.314 * T / (2 * 96485)) * ln(ilim / (ilim - i))
p.V - n.V = ncells * V_cell
p.I + n.I = 0
Q         = I_cell * ncells * (Eth - V_cell)
heat.Qdot = -Q
```
