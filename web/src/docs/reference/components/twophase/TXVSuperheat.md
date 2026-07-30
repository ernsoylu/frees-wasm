---
name: TXVSuperheat
category: Component (twophase)
summary: A thermostatic expansion valve that meters flow to hold a target superheat.
related: []
examples: []
tags: [txvsuperheat, component, twophase, acausal]
---

# TXVSuperheat

A thermostatic expansion valve that meters flow to hold a target superheat.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`, `bulb`

## Usage

```
TXVSuperheat inst(fluid$, Kv, SH_set, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `Kv` | Number | Flow coefficient. |
| `SH_set` | Number | Target superheat [K]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot  = in.mdot
out.h     = in.h
bulb.Qdot = 0
T_sat     = Temperature(fluid$, P=out.P, x=1)
SH        = bulb.T - T_sat
in.mdot   = Kv * (SH - SH_set)
```
