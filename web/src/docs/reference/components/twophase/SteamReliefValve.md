---
name: SteamReliefValve
category: Component (twophase)
summary: A steam relief valve venting above the set pressure.
related: []
examples: []
tags: [steamreliefvalve, component, twophase, acausal]
---

# SteamReliefValve

A steam relief valve venting above the set pressure.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
SteamReliefValve inst(fluid$, A, Pset, Cd, kgas, Rgas, eps, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `A` | Number | Area [m²]. |
| `Pset` | Number | Set pressure [Pa]. |
| `Cd` | Number | Discharge coefficient. |
| `kgas` | Number | Gas specific-heat ratio. |
| `Rgas` | Number | Specific gas constant [J/kg·K]. |
| `eps` | Number | Effectiveness / roughness. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.h    = in.h
opening  = 0.5 * (1 + tanh((in.P - Pset) / eps))
T0v      = Temperature(fluid$, P=in.P, x=1)
PRc      = (2 / (kgas + 1)) ^ (kgas / (kgas - 1))
mdot_ch  = Cd * A * in.P * sqrt(kgas / (Rgas * T0v)) * (2 / (kgas + 1)) ^ ((kgas + 1) / (2 * (kgas - 1)))
ratio    = (min(max(out.P / in.P, PRc), 1) - PRc) / (1 - PRc)
efact    = 1 - ratio ^ 2
in.mdot  = opening * mdot_ch * efact
```
