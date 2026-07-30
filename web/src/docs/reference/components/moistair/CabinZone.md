---
name: CabinZone
category: Component (moistair)
summary: Acausal moistair-domain component CabinZone with ports in, out, wall.
related: []
examples: []
tags: [cabinzone, component, moistair, acausal]
references: []
generated: true
---

# CabinZone

Reusable acausal **moistair-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
CabinZone inst(Vz, T0, W0, n_occ, q_sens, mw_occ, Q_aux, domain$)
```

## Ports

`in`, `out`, `wall`

## Parameters

| Parameter | Type |
| --- | --- |
| `Vz` | Number |
| `T0` | Number |
| `W0` | Number |
| `n_occ` | Number |
| `q_sens` | Number |
| `mw_occ` | Number |
| `Q_aux` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot  = in.mdot
out.P     = in.P
out.W     = Wz
out.h     = Enthalpy(AirH2O, T=Tz, P=in.P, W=Wz)
v_z       = Volume(AirH2O, T=Tz, P=in.P, W=Wz)
cp_z      = Cp(AirH2O, T=Tz, P=in.P, W=Wz)
m_air     = Vz / v_z
der(Wz)   = (in.mdot * (in.W - Wz) + n_occ * mw_occ) / m_air
init(Wz)  = W0
der(Tz)   = (in.mdot * (in.h - out.h) + n_occ * q_sens + Q_aux + wall.Qdot) / (m_air * cp_z)
init(Tz)  = T0
wall.T    = Tz
```
