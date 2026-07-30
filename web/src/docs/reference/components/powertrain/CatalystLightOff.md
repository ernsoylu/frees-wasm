---
name: CatalystLightOff
category: Component (powertrain)
summary: Acausal powertrain-domain component CatalystLightOff with ports in, out.
related: []
examples: []
tags: [catalystlightoff, component, powertrain, acausal]
references: []
generated: true
---

# CatalystLightOff

Reusable acausal **powertrain-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
CatalystLightOff inst(fluid$, C, UA, T50, k, q_exo, T0)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `C` | Number |
| `UA` | Number |
| `T50` | Number |
| `k` | Number |
| `q_exo` | Number |
| `T0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
T_g      = Temperature(fluid$, P=in.P, h=in.h)
eta      = 0.5 * (1 + tanh((Tb - T50) / k))
Q        = UA * (T_g - Tb)
Qexo     = eta * in.mdot * q_exo
der(Tb)  = (Q + Qexo) / C
init(Tb) = T0
out.mdot = in.mdot
out.P    = in.P
out.h    = in.h - Q / in.mdot
```
