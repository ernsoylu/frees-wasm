---
name: Conduction
category: Component (heat)
summary: A conductive thermal resistance (Fourier), Q̇ = (T1 − T2)/R.
related: []
examples: [heat-conduction, transient-heat-rod, heisler-transient, material-conduction]
tags: [conduction, component, heat, acausal]
---

# Conduction

A conductive thermal resistance (Fourier), `Q̇ = (T1 − T2)/R`.

## Domain

A reusable **acausal heat-domain** component — its thermal ports carry temperature `T` and heat-flow rate `Q̇`; a node enforces equal `T` and `ΣQ̇ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`a`, `b`

## Usage

```
Conduction inst(k, area, L)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `k` | Number | Stiffness / conductivity. |
| `area` | Number | Area [m²]. |
| `L` | Number | Length [m]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
Q      = k * area / L * (a.T - b.T)
a.Qdot = Q
b.Qdot = -Q
```

## Examples

Instantiated in the verified example below:

[Run: heat-conduction]
