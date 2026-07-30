---
name: Turbocharger
category: Component (fluid)
summary: A turbine-driven compressor pair coupled on a common shaft.
related: []
examples: []
tags: [turbocharger, component, fluid, acausal]
---

# Turbocharger

A turbine-driven compressor pair coupled on a common shaft.

## Domain

A reusable **acausal fluid-domain** component — its thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`t_in`, `t_out`, `c_in`, `c_out`

## Usage

```
Turbocharger inst(cp, eta_t, eta_c, gam)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `cp` | Number | Specific heat [J/kg·K]. |
| `eta_t` | Number | Turbine efficiency (0–1). |
| `eta_c` | Number | Compressor efficiency (0–1). |
| `gam` | Number | Ratio of specific heats. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
PRt        = t_in.P / t_out.P
t_out.T    = t_in.T * (1 - eta_t * (1 - PRt^((1 - gam) / gam)))
t_out.mdot = t_in.mdot
Wt         = t_in.mdot * cp * (t_in.T - t_out.T)
PRc        = c_out.P / c_in.P
c_out.T    = c_in.T * (1 + (PRc^((gam - 1) / gam) - 1) / eta_c)
c_out.mdot = c_in.mdot
Wc         = c_in.mdot * cp * (c_out.T - c_in.T)
Wt         = Wc
```
