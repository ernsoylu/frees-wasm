---
name: FlashTank
category: Component (twophase)
summary: Acausal twophase-domain component FlashTank with ports in, liq, vap.
related: []
examples: []
tags: [flashtank, component, twophase, acausal]
references: []
generated: true
---

# FlashTank

Reusable acausal **twophase-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
FlashTank inst(fluid$, domain$)
```

## Ports

`in`, `liq`, `vap`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
liq.P = in.P
vap.P = in.P
liq.h = Enthalpy(fluid$, P=in.P, x=0)
vap.h = Enthalpy(fluid$, P=in.P, x=1)
in.mdot = liq.mdot + vap.mdot
in.mdot * in.h = liq.mdot * liq.h + vap.mdot * vap.h
```
