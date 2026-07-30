---
name: Pipe
category: Component (fluid)
summary: A flow passage that imposes a frictional pressure drop.
related: []
examples: []
tags: [pipe, component, fluid, acausal]
---

# Pipe

A flow passage that imposes a frictional pressure drop.

## Domain

A reusable **acausal fluid-domain** component — its thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
Pipe inst(fluid$, L, D, rough)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `L` | Number | Length [m]. |
| `D` | Number | Diameter [m]. |
| `rough` | Number | Relative wall roughness. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.h    = in.h
rho      = Density(fluid$, P=in.P, h=in.h)
mu       = Viscosity(fluid$, P=in.P, h=in.h)
A        = pi# / 4 * D^2
V        = in.mdot / (rho * A)
Re_d     = reynolds(rho, V, D, mu)
f        = friction_factor(Re_d, rough / D)
out.P    = in.P - f * (L / D) * rho * V^2 / 2
```
