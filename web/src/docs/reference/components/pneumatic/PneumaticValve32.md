---
name: PneumaticValve32
category: Component (pneumatic)
summary: Acausal pneumatic-domain component PneumaticValve32 with ports sup_in, work, exh_out, u.
related: []
examples: []
tags: [pneumaticvalve32, component, pneumatic, acausal]
references: []
generated: true
---

# PneumaticValve32

Reusable acausal **pneumatic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
PneumaticValve32 inst(fluid$, C, b, domain$)
```

## Ports

`sup_in`, `work`, `exh_out`, `u`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `C` | Number |
| `b` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
T_s          = Temperature(fluid$, P=sup_in.P, h=sup_in.h)
T_w          = Temperature(fluid$, P=work.P, h=work.h)
m_in         = iso6358(u.sig * C, b, sup_in.P, T_s, work.P)
m_out        = iso6358((1 - u.sig) * C, b, work.P, T_w, exh_out.P)
sup_in.mdot  = m_in
work.mdot    = m_in - m_out
exh_out.mdot = m_out
work.h       = sup_in.h
exh_out.h    = work.h
```
