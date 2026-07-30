---
name: MoistAirFan
category: Component (moistair)
summary: Acausal moistair-domain component MoistAirFan with ports in, out.
related: []
examples: []
tags: [moistairfan, component, moistair, acausal]
references: []
generated: true
---

# MoistAirFan

Reusable acausal **moistair-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
MoistAirFan inst(dP, eta, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `dP` | Number |
| `eta` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.W    = in.W
out.P    = in.P + dP
T_in     = Temperature(AirH2O, h=in.h, P=in.P, W=in.W)
v_in     = Volume(AirH2O, T=T_in, P=in.P, W=in.W)
out.h    = in.h + v_in * dP / eta
W_el     = in.mdot * v_in * dP / eta
```
