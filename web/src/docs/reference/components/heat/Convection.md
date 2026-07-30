---
name: Convection
category: Component (heat)
summary: A convective link (Newton’s law of cooling), Q̇ = h·A·ΔT.
related: []
examples: [pressure-cooker]
tags: [convection, component, heat, acausal]
---

# Convection

A convective link (Newton’s law of cooling), `Q̇ = h·A·ΔT`.

## Domain

A reusable **acausal heat-domain** component — its thermal ports carry temperature `T` and heat-flow rate `Q̇`; a node enforces equal `T` and `ΣQ̇ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`a`, `b`

## Usage

```
Convection inst(htc, area)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `htc` | Number | Heat-transfer coefficient [W/m²·K]. |
| `area` | Number | Area [m²]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
Q      = htc * area * (a.T - b.T)
a.Qdot = Q
b.Qdot = -Q
```

## Examples

Instantiated in the verified example below:

[Run: pressure-cooker]
