---
name: ThermalSource
category: Component (heat)
summary: A prescribed-temperature boundary.
related: []
examples: [ev-thermal-management]
tags: [thermalsource, component, heat, acausal]
---

# ThermalSource

A prescribed-temperature boundary.

## Domain

A reusable **acausal heat-domain** component — its thermal ports carry temperature `T` and heat-flow rate `Q̇`; a node enforces equal `T` and `ΣQ̇ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`port`

## Usage

```
ThermalSource inst(T)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `T` | Number | Temperature [K]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
port.T = T
```

## Examples

Instantiated in the verified example below:

[Run: ev-thermal-management]
