---
name: CoolingTower
category: Component (liquid)
summary: Acausal liquid-domain component CoolingTower with ports in, out, wb.
related: []
examples: []
tags: [coolingtower, component, liquid, acausal]
references: []
generated: true
---

# CoolingTower

Reusable acausal **liquid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
CoolingTower inst(fluid$, eps_t, mdot_a, Patm, domain$)
```

## Ports

`in`, `out`, `wb`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `eps_t` | Number |
| `mdot_a` | Number |
| `Patm` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
T_in   = Temperature(fluid$, P=in.P, h=in.h)
h_s_in = Enthalpy(AirH2O, T=T_in, P=Patm, R=1)
h_wb   = Enthalpy(AirH2O, T=wb.sig, P=Patm, R=1)
Q      = eps_t * mdot_a * (h_s_in - h_wb)
out.mdot = in.mdot
out.P    = in.P
out.h    = in.h - Q / in.mdot
```
