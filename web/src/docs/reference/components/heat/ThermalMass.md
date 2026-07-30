---
name: ThermalMass
category: Component (heat)
summary: A lumped thermal capacitance, C dT/dt = Q̇.
related: []
examples: [pressure-cooker]
tags: [thermalmass, component, heat, acausal]
---

# ThermalMass

A lumped thermal capacitance, `C dT/dt = Q̇`.

## Domain

A reusable **acausal heat-domain** component — its thermal ports carry temperature `T` and heat-flow rate `Q̇`; a node enforces equal `T` and `ΣQ̇ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`port`

## Usage

```
ThermalMass inst(C, T0)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `C` | Number | Capacitance [F]. |
| `T0` | Number | Reference/initial temperature [K]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
der(port.T)  = port.Qdot / C
init(port.T) = T0
```

## Examples

Instantiated in the verified example below:

[Run: pressure-cooker]
