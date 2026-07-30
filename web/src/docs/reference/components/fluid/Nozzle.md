---
name: Nozzle
category: Component (fluid)
summary: Accelerates a flow, converting enthalpy into kinetic energy.
related: []
examples: [cd-nozzle-shock]
tags: [nozzle, component, fluid, acausal]
---

# Nozzle

Accelerates a flow, converting enthalpy into kinetic energy.

## Domain

A reusable **acausal fluid-domain** component — its thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
Nozzle inst(k, R, A_throat, A_exit, P_amb, T0)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `k` | Number | Stiffness / conductivity. |
| `R` | Number | Resistance [Ω]. |
| `A_throat` | Number | Throat area [m²]. |
| `A_exit` | Number | Exit area [m²]. |
| `P_amb` | Number | Ambient pressure [Pa]. |
| `T0` | Number | Reference/initial temperature [K]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
M_exit   = mach_A_Astar(A_exit / A_throat, k, 'supersonic')
out.P    = in.P / P0_P(M_exit, k)
T_exit   = T0 / T0_T(M_exit, k)
V_exit   = M_exit * sqrt(k * R * T_exit)
out.h    = in.h - V_exit^2 / 2
thrust   = in.mdot * V_exit + (out.P - P_amb) * A_exit
```

## Examples

Instantiated in the verified example below:

[Run: cd-nozzle-shock]
