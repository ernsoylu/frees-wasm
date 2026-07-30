---
name: HydraulicPump
category: Component (hydraulic)
summary: A hydraulic pump delivering flow against pressure.
related: []
examples: []
tags: [hydraulicpump, component, hydraulic, acausal]
---

# HydraulicPump

A hydraulic pump delivering flow against pressure.

## Domain

A reusable **acausal hydraulic-domain** component — its oil-hydraulic ports carry pressure `P`, mass-flow `ṁ`, and enthalpy `h`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`, `shaft`

## Usage

```
HydraulicPump inst(disp, rho, eta_v, eta_m, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `disp` | Number | Displacement volume [m³]. |
| `rho` | Number | Density [kg/m³]. |
| `eta_v` | Number | Volumetric efficiency (0–1). |
| `eta_m` | Number | Mechanical efficiency (0–1). |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
n_rev     = shaft.w / (2 * pi#)
out.mdot  = rho * disp * n_rev * eta_v
in.mdot   = out.mdot
out.h     = in.h
shaft.tau = -(disp * (out.P - in.P) / (2 * pi#)) / eta_m
```
