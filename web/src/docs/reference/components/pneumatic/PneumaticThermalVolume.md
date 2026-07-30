---
name: PneumaticThermalVolume
category: Component (pneumatic)
summary: Acausal pneumatic-domain component PneumaticThermalVolume with ports in, out, wall.
related: []
examples: []
tags: [pneumaticthermalvolume, component, pneumatic, acausal]
references: []
generated: true
---

# PneumaticThermalVolume

Reusable acausal **pneumatic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
PneumaticThermalVolume inst(fluid$, V, R, cv, cp, m0, T0, domain$)
```

## Ports

`in`, `out`, `wall`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `V` | Number |
| `R` | Number |
| `cv` | Number |
| `cp` | Number |
| `m0` | Number |
| `T0` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(m)  = in.mdot - out.mdot
init(m) = m0
T_in    = Temperature(fluid$, P=in.P, h=in.h)
der(T)  = (in.mdot * cp * (T_in - T) + wall.Qdot) / (m * cv)
init(T) = T0
in.P    = m * R * T / V
out.P   = in.P
out.h   = Enthalpy(fluid$, P=out.P, T=T)
wall.T  = T
```
