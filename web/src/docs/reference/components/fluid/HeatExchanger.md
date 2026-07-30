---
name: HeatExchanger
category: Component (fluid)
summary: Transfers heat between two fluid streams across a wall.
related: []
examples: []
tags: [heatexchanger, component, fluid, acausal]
---

# HeatExchanger

Transfers heat between two fluid streams across a wall.

## Domain

A reusable **acausal fluid-domain** component — its thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`hot_in`, `hot_out`, `cold_in`, `cold_out`

## Usage

```
HeatExchanger inst(UA, hot$, cold$, arr$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `UA` | Number | Overall conductance UA [W/K]. |
| `hot$` | String | Hot-side fluid name (e.g. Water). |
| `cold$` | String | Cold-side fluid name (e.g. EG50). |
| `arr$` | String | Flow arrangement (passed to hx_effectiveness) — one of `counterflow`, `parallel`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
hot_out.mdot  = hot_in.mdot
hot_out.P     = hot_in.P
cold_out.mdot = cold_in.mdot
cold_out.P    = cold_in.P
Th   = Temperature(hot$,  P=hot_in.P,  h=hot_in.h)
Tc   = Temperature(cold$, P=cold_in.P, h=cold_in.h)
C_h  = hot_in.mdot  * Cp(hot$,  P=hot_in.P,  h=hot_in.h)
C_c  = cold_in.mdot * Cp(cold$, P=cold_in.P, h=cold_in.h)
Cmin = min(C_h, C_c)
Cmax = max(C_h, C_c)
eps  = hx_effectiveness(arr$, UA / Cmin, Cmin / Cmax)
Q    = eps * Cmin * (Th - Tc)
hot_out.h  = hot_in.h  - Q / hot_in.mdot
cold_out.h = cold_in.h + Q / cold_in.mdot
```
