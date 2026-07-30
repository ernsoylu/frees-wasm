---
name: ThermalStorageTank
category: Component (liquid)
summary: Acausal liquid-domain component ThermalStorageTank with ports in, out.
related: []
examples: []
tags: [thermalstoragetank, component, liquid, acausal]
references: []
generated: true
---

# ThermalStorageTank

Reusable acausal **liquid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
ThermalStorageTank inst(fluid$, m_node, cp_f, UA_loss, T_amb, kmix, T10, T20, T30, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `m_node` | Number |
| `cp_f` | Number |
| `UA_loss` | Number |
| `T_amb` | Number |
| `kmix` | Number |
| `T10` | Number |
| `T20` | Number |
| `T30` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.P    = in.P
T_in     = Temperature(fluid$, P=in.P, h=in.h)
der(T1)  = (in.mdot * cp_f * (T_in - T1) + kmix * (T2 - T1) - UA_loss * (T1 - T_amb)) / (m_node * cp_f)
init(T1) = T10
der(T2)  = (in.mdot * cp_f * (T1 - T2) + kmix * (T1 - T2) + kmix * (T3 - T2) - UA_loss * (T2 - T_amb)) / (m_node * cp_f)
init(T2) = T20
der(T3)  = (in.mdot * cp_f * (T2 - T3) + kmix * (T2 - T3) - UA_loss * (T3 - T_amb)) / (m_node * cp_f)
init(T3) = T30
out.h    = Enthalpy(fluid$, P=out.P, T=T3)
```
