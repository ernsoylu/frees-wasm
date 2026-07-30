---
name: TwoPhaseChamber
category: Component (twophase)
summary: A two-phase control volume.
related: []
examples: []
tags: [twophasechamber, component, twophase, acausal]
---

# TwoPhaseChamber

A two-phase control volume.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`, `wall`

## Usage

```
TwoPhaseChamber inst(fluid$, V, C, UA, P0, h0, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `V` | Number | Volume [m³]. |
| `C` | Number | Capacitance [F]. |
| `UA` | Number | Overall conductance UA [W/K]. |
| `P0` | Number | Reference/initial pressure [Pa]. |
| `h0` | Number | Reference enthalpy [J/kg]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
der(in.P)  = (in.mdot - out.mdot) / C
init(in.P) = P0
rho        = Density(fluid$, P=in.P, h=hcv)
Q          = UA * (wall.T - Tcv)
der(hcv)   = (in.mdot * (in.h - hcv) + Q) / (rho * V)
init(hcv)  = h0
out.P     = in.P
out.h     = hcv
Tcv       = Temperature(fluid$, P=in.P, h=hcv)
wall.Qdot = Q
m         = rho * V
```
