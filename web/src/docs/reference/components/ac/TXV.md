---
name: TXV
category: Component (ac)
summary: A thermostatic expansion valve that meters refrigerant to hold a target superheat.
related: []
examples: []
tags: [txv, component, ac, acausal]
---

# TXV

A thermostatic expansion valve that meters refrigerant to hold a target superheat.

## Domain

A reusable **acausal ac-domain** component — its refrigerant/air ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`, `bulb`

## Usage

```
TXV inst(fluid$, Kv, SH_set, CdA0, tau_valve, tau_bulb, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `Kv` | Number | Flow coefficient. |
| `SH_set` | Number | Target superheat [K]. |
| `CdA0` | Number | Reference Cd·A [m²]. |
| `tau_valve` | Number | Valve time constant [s]. |
| `tau_bulb` | Number | Bulb time constant [s]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot   = in.mdot
out.h      = in.h
bulb.Qdot  = 0
Tsat       = T_sat(fluid$, P=out.P)
SH_sensed  = bulb.T - Tsat
der(SH_b)  = (SH_sensed - SH_b) / tau_bulb
init(SH_b) = SH_set
CdA_t      = CdA0 + Kv * (SH_b - SH_set)
der(CdA)   = (CdA_t - CdA) / tau_valve
init(CdA)  = CdA0
rho_in     = Density(fluid$, P=in.P, h=in.h)
in.mdot * abs(in.mdot) = CdA^2 * 2 * rho_in * (in.P - out.P)
```
