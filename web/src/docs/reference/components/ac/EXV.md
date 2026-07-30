---
name: EXV
category: Component (ac)
summary: An electronic expansion valve with a commanded opening.
related: []
examples: []
tags: [exv, component, ac, acausal]
---

# EXV

An electronic expansion valve with a commanded opening.

## Domain

A reusable **acausal ac-domain** component — its refrigerant/air ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
EXV inst(fluid$, CdA_max, u, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `CdA_max` | Number | Maximum Cd·A [m²]. |
| `u` | Number | Specific internal energy [J/kg]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.h    = in.h
rho_in   = Density(fluid$, P=in.P, h=in.h)
in.mdot * abs(in.mdot) = (u * CdA_max)^2 * 2 * rho_in * (in.P - out.P)
```
