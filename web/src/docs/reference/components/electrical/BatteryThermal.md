---
name: BatteryThermal
category: Component (electrical)
summary: A battery with a coupled thermal model relating losses to temperature.
related: []
examples: []
tags: [batterythermal, component, electrical, acausal]
---

# BatteryThermal

A battery with a coupled thermal model relating losses to temperature.

## Domain

A reusable **acausal electrical-domain** component — its electrical ports carry potential `V` and current `I`; a node enforces equal `V` and `ΣI = 0` (Kirchhoff). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`p`, `n`, `heat`

## Usage

```
BatteryThermal inst(Voc, R0)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `Voc` | Number | Open-circuit voltage [V]. |
| `R0` | Number | Series (ohmic) resistance [Ω]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
p.V - n.V = Voc + R0 * p.I
p.I + n.I = 0
Q         = R0 * p.I^2
heat.Qdot = -Q
W         = (p.V - n.V) * (0 - p.I)
```
