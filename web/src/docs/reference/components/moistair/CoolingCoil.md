---
name: CoolingCoil
category: Component (moistair)
summary: Cools and (below dew point) dehumidifies a humid-air stream.
related: []
examples: []
tags: [coolingcoil, component, moistair, acausal]
---

# CoolingCoil

Cools and (below dew point) dehumidifies a humid-air stream.

## Domain

A reusable **acausal moistair-domain** component — its humid-air ports carry pressure `P`, dry-air mass-flow `ṁ_da`, enthalpy `h`, and humidity ratio `W`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
CoolingCoil inst(Tout, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `Tout` | Number | Outlet temperature [K]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.P    = in.P
out.W    = HumRat(AirH2O, T=Tout, P=in.P, R=1)
out.h    = Enthalpy(AirH2O, T=Tout, P=in.P, W=out.W)
Q        = in.mdot * (in.h - out.h)
Q_lat    = in.mdot * 2.501e6 * (in.W - out.W)
```
