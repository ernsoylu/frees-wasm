---
name: HeatSource
category: Component (heat)
summary: A prescribed heat input to a thermal node.
related: []
examples: []
tags: [heatsource, component, heat, acausal]
---

# HeatSource

A prescribed heat input to a thermal node.

## Domain

A reusable **acausal heat-domain** component — its thermal ports carry temperature `T` and heat-flow rate `Q̇`; a node enforces equal `T` and `ΣQ̇ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`port`

## Usage

```
HeatSource inst(Q)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `Q` | Number | Heat input [W]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
port.Qdot = -Q
```
