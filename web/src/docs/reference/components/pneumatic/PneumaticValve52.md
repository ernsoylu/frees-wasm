---
name: PneumaticValve52
category: Component (pneumatic)
summary: Acausal pneumatic-domain component PneumaticValve52 with ports sup_in, wa, wb, ea_out, eb_out, u.
related: []
examples: []
tags: [pneumaticvalve52, component, pneumatic, acausal]
references: []
generated: true
---

# PneumaticValve52

Reusable acausal **pneumatic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
PneumaticValve52 inst(fluid$, C, b, domain$)
```

## Ports

`sup_in`, `wa`, `wb`, `ea_out`, `eb_out`, `u`

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
T_s         = Temperature(fluid$, P=sup_in.P, h=sup_in.h)
T_a         = Temperature(fluid$, P=wa.P, h=wa.h)
T_b         = Temperature(fluid$, P=wb.P, h=wb.h)
m_sa        = iso6358(u.sig * C, b, sup_in.P, T_s, wa.P)
m_be        = iso6358(u.sig * C, b, wb.P, T_b, eb_out.P)
m_sb        = iso6358((1 - u.sig) * C, b, sup_in.P, T_s, wb.P)
m_ae        = iso6358((1 - u.sig) * C, b, wa.P, T_a, ea_out.P)
sup_in.mdot = m_sa + m_sb
wa.mdot     = m_sa - m_ae
wb.mdot     = m_sb - m_be
ea_out.mdot = m_ae
eb_out.mdot = m_be
wa.h        = sup_in.h
wb.h        = sup_in.h
ea_out.h    = wa.h
eb_out.h    = wb.h
```
