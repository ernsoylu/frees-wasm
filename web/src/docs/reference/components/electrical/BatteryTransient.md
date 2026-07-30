---
name: BatteryTransient
category: Component (electrical)
summary: A transient battery model carrying state-of-charge dynamics.
related: []
examples: []
tags: [batterytransient, component, electrical, acausal]
---

# BatteryTransient

A transient battery model carrying state-of-charge dynamics.

## Domain

A reusable **acausal electrical-domain** component — its electrical ports carry potential `V` and current `I`; a node enforces equal `V` and `ΣI = 0` (Kirchhoff). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`p`, `n`, `heat`

## Usage

```
BatteryTransient inst(Voc, R0, Q0, C_th, SOC0, T0)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `Voc` | Number | Open-circuit voltage [V]. |
| `R0` | Number | Series (ohmic) resistance [Ω]. |
| `Q0` | Number | Reference heat [W]. |
| `C_th` | Number | Thermal capacitance [J/K]. |
| `SOC0` | Number | Initial state of charge (0–1). |
| `T0` | Number | Reference/initial temperature [K]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
p.V - n.V = Voc + R0 * p.I
p.I + n.I = 0
Qgen      = R0 * p.I^2
heat.T    = T
der(T)    = (Qgen + heat.Qdot) / C_th
init(T)   = T0
der(SOC)  = p.I / (3600 * Q0)
init(SOC) = SOC0
```
