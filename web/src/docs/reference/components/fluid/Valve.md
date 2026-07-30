---
name: Valve
category: Component (fluid)
summary: A flow restriction characterized by a flow/pressure-drop coefficient.
related: []
examples: []
tags: [valve, component, fluid, acausal]
---

# Valve

A flow restriction characterized by a flow/pressure-drop coefficient.

## Domain

A reusable **acausal fluid-domain** component — its thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
Valve inst(Cv, rho)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `Cv` | Number | Flow coefficient. |
| `rho` | Number | Density [kg/m³]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.h    = in.h
in.mdot * abs(in.mdot) = Cv^2 * rho * (in.P - out.P)
```
