---
name: Duct
category: Component (fluid)
summary: A flow passage that imposes a pressure drop on the stream.
related: []
examples: []
tags: [duct, component, fluid, acausal]
---

# Duct

A flow passage that imposes a pressure drop on the stream.

## Domain

A reusable **acausal fluid-domain** component — its thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
Duct inst(rho, mu, L, D, rough)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `rho` | Number | Density [kg/m³]. |
| `mu` | Number | Dynamic viscosity [Pa·s]. |
| `L` | Number | Length [m]. |
| `D` | Number | Diameter [m]. |
| `rough` | Number | Relative wall roughness. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
A        = pi# / 4 * D^2
V        = in.mdot / (rho * A)
Re_d     = reynolds(rho, V, D, mu)
f        = friction_factor(Re_d, rough / D)
out.P    = in.P - f * (L / D) * rho * V^2 / 2
```
