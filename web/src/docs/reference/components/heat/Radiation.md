---
name: Radiation
category: Component (heat)
summary: A radiative exchange link (Stefan–Boltzmann), Q̇ = εσA(T1⁴ − T2⁴).
related: []
examples: [radiation-view-factors]
tags: [radiation, component, heat, acausal]
---

# Radiation

A radiative exchange link (Stefan–Boltzmann), `Q̇ = εσA(T1⁴ − T2⁴)`.

## Domain

A reusable **acausal heat-domain** component — its thermal ports carry temperature `T` and heat-flow rate `Q̇`; a node enforces equal `T` and `ΣQ̇ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`a`, `b`

## Usage

```
Radiation inst(emis, area)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `emis` | Number | Emissivity (0–1). |
| `area` | Number | Area [m²]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
Q      = emis * 5.670374419e-8 * area * (a.T^4 - b.T^4)
a.Qdot = Q
b.Qdot = -Q
```

## Examples

Instantiated in the verified example below:

[Run: radiation-view-factors]
