---
name: LiquidOrifice
category: Component (liquid)
summary: A liquid orifice metering flow vs. pressure drop.
related: []
examples: [ev-thermal-management]
tags: [liquidorifice, component, liquid, acausal]
---

# LiquidOrifice

A liquid orifice metering flow vs. pressure drop.

## Domain

A reusable **acausal liquid-domain** component — its single-phase liquid-coolant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
LiquidOrifice inst(CdA, rho, domain$, model$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `CdA` | Number | Discharge coefficient × area Cd·A [m²]. |
| `rho` | Number | Density [kg/m³]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |
| `model$` | String | Model variant — selects the physics body (see Model Variants). |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.h    = in.h
```

## Model Variants

Selected via the `model$` parameter; each adds its own equations (and `REQUIRE`d parameters):

### `incompressible`

```
in.mdot * abs(in.mdot) = CdA^2 * 2 * rho * (in.P - out.P)
```

### `cavitating` — requires `Pvap`

```
dP_eff = in.P - max(out.P, Pvap)
in.mdot * abs(in.mdot) = CdA^2 * 2 * rho * dP_eff
```

## Examples

Instantiated in the verified example below:

[Run: ev-thermal-management]
