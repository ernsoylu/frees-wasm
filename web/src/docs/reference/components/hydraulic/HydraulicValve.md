---
name: HydraulicValve
category: Component (hydraulic)
summary: A hydraulic valve metering flow vs. pressure drop.
related: []
examples: []
tags: [hydraulicvalve, component, hydraulic, acausal]
---

# HydraulicValve

A hydraulic valve metering flow vs. pressure drop.

## Domain

A reusable **acausal hydraulic-domain** component — its oil-hydraulic ports carry pressure `P`, mass-flow `ṁ`, and enthalpy `h`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
HydraulicValve inst(CdA_max, rho, u, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `CdA_max` | Number | Maximum Cd·A [m²]. |
| `rho` | Number | Density [kg/m³]. |
| `u` | Number | Specific internal energy [J/kg]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.h    = in.h
in.mdot * abs(in.mdot) = (u * CdA_max)^2 * 2 * rho * (in.P - out.P)
```
