---
name: BoilingVessel
category: Component (twophase)
summary: A rigid vessel boiling a two-phase fluid (rigid two-phase boil-off).
related: []
examples: [pressure-cooker]
tags: [boilingvessel, component, twophase, acausal]
---

# BoilingVessel

A rigid vessel boiling a two-phase fluid (rigid two-phase boil-off).

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`vent`, `wall`

## Usage

```
BoilingVessel inst(fluid$, V, m0, T0, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `V` | Number | Volume [m³]. |
| `m0` | Number | Initial mass [kg]. |
| `T0` | Number | Reference/initial temperature [K]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
der(mass)  = -vent.mdot
init(mass) = m0
der(Utot)  = wall.Qdot - vent.mdot * vent.h
init(Utot) = m0 * IntEnergy(fluid$, T=T0, x=0)
rho_cv = mass / V
u_cv   = Utot / mass
vent.P = Pressure(fluid$, d=rho_cv, u=u_cv)      { (rho,u) flash -> pressure }
T_cv   = Temperature(fluid$, d=rho_cv, u=u_cv)
x_cv   = Quality(fluid$, d=rho_cv, u=u_cv)        { vapour mass fraction }
vent.h = Enthalpy(fluid$, P=vent.P, x=1)          { vented stream is sat. vapour }
wall.T = T_cv
```

## Examples

Instantiated in the verified example below:

[Run: pressure-cooker]
