---
name: TwoPhaseInternalHX
category: Component (twophase)
summary: Acausal twophase-domain component TwoPhaseInternalHX with ports liq_in, liq_out, vap_in, vap_out.
related: []
examples: []
tags: [twophaseinternalhx, component, twophase, acausal]
references: []
generated: true
---

# TwoPhaseInternalHX

Reusable acausal **twophase-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
TwoPhaseInternalHX inst(fluid$, eps, domain$)
```

## Ports

`liq_in`, `liq_out`, `vap_in`, `vap_out`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `eps` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
liq_out.mdot = liq_in.mdot
vap_out.mdot = vap_in.mdot
liq_out.P    = liq_in.P
vap_out.P    = vap_in.P
T_liq        = Temperature(fluid$, P=liq_in.P, h=liq_in.h)
T_vap        = Temperature(fluid$, P=vap_in.P, h=vap_in.h)
cp_v         = Cp(fluid$, P=vap_in.P, h=vap_in.h)
Q            = eps * vap_in.mdot * cp_v * (T_liq - T_vap)
vap_out.h    = vap_in.h + Q / vap_in.mdot
liq_out.h    = liq_in.h - Q / liq_in.mdot
```
