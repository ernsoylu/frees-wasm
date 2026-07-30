---
name: HydraulicOrifice
category: Component (hydraulic)
summary: A hydraulic orifice metering flow by ṁ ∝ √Δp.
related: []
examples: []
tags: [hydraulicorifice, component, hydraulic, acausal]
---

# HydraulicOrifice

A hydraulic orifice metering flow by `ṁ ∝ √Δp`.

## Domain

A reusable **acausal hydraulic-domain** component — its oil-hydraulic ports carry pressure `P`, mass-flow `ṁ`, and enthalpy `h`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
HydraulicOrifice inst(CdA, rho, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `CdA` | Number | Discharge coefficient × area Cd·A [m²]. |
| `rho` | Number | Density [kg/m³]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.h    = in.h
in.mdot * abs(in.mdot) = CdA^2 * 2 * rho * (in.P - out.P)
```
