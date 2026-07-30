---
name: ThermalSensor
category: Component (heat)
summary: A temperature sensor (pass-through).
related: []
examples: []
tags: [thermalsensor, component, heat, acausal]
---

# ThermalSensor

A temperature sensor (pass-through).

## Domain

A reusable **acausal heat-domain** component — its thermal ports carry temperature `T` and heat-flow rate `Q̇`; a node enforces equal `T` and `ΣQ̇ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`port`

## Usage

```
ThermalSensor inst(...)
```

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
port.Qdot = 0
T_meas    = port.T
```
