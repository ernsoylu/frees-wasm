---
name: BatteryPack
category: Component (electrical)
summary: Acausal electrical-domain component BatteryPack with ports p, n, heat.
related: []
examples: []
tags: [batterypack, component, electrical, acausal]
references: []
generated: true
---

# BatteryPack

Reusable acausal **electrical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
BatteryPack inst(Ns, Np, ocv$, dudt$, R0ref, Tref, Ea, Q0, C_th, SOC0, T0)
```

## Ports

`p`, `n`, `heat`

## Parameters

| Parameter | Type |
| --- | --- |
| `Ns` | Number |
| `Np` | Number |
| `ocv$` | String |
| `dudt$` | String |
| `R0ref` | Number |
| `Tref` | Number |
| `Ea` | Number |
| `Q0` | Number |
| `C_th` | Number |
| `SOC0` | Number |
| `T0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
I_cell    = -p.I / Np
R0        = R0ref * exp((Ea / 8.314) * (1 / T - 1 / Tref))
p.V - n.V = Ns * (ocv$(SOC) - R0 * I_cell)
p.I + n.I = 0
der(SOC)  = -I_cell / (3600 * Q0)
init(SOC) = SOC0
Qgen      = Ns * Np * (R0 * I_cell^2 - I_cell * T * dudt$(SOC))
heat.T    = T
der(T)    = (Qgen + heat.Qdot) / C_th
init(T)   = T0
```
