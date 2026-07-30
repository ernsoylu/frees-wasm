---
name: HydraulicCylinder
category: Component (hydraulic)
summary: A hydraulic actuator converting flow/pressure to motion/force.
related: []
examples: []
tags: [hydrauliccylinder, component, hydraulic, acausal]
---

# HydraulicCylinder

A hydraulic actuator converting flow/pressure to motion/force.

## Domain

A reusable **acausal hydraulic-domain** component — its oil-hydraulic ports carry pressure `P`, mass-flow `ṁ`, and enthalpy `h`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `rod`

## Usage

```
HydraulicCylinder inst(rho, beta, V0, area, Patm, P0, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `rho` | Number | Density [kg/m³]. |
| `beta` | Number | Chevron angle [deg] / coefficient. |
| `V0` | Number | Initial voltage / volume. |
| `area` | Number | Area [m²]. |
| `Patm` | Number | Atmospheric pressure [Pa]. |
| `P0` | Number | Reference/initial pressure [Pa]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
rod.f      = -(in.P - Patm) * area
der(in.P)  = (beta / V0) * (in.mdot / rho - area * rod.vel)
init(in.P) = P0
```
