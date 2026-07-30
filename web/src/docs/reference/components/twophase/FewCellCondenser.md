---
name: FewCellCondenser
category: Component (twophase)
summary: Acausal twophase-domain component FewCellCondenser with ports in, out, w1, w2, w3.
related: []
examples: []
tags: [fewcellcondenser, component, twophase, acausal]
references: []
generated: true
---

# FewCellCondenser

Reusable acausal **twophase-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
FewCellCondenser inst(fluid$, V, Cc, UA, Kv, P0, h0, domain$)
```

## Ports

`in`, `out`, `w1`, `w2`, `w3`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `V` | Number |
| `Cc` | Number |
| `UA` | Number |
| `Kv` | Number |
| `P0` | Number |
| `h0` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
in.mdot  = Kv * (in.P - P1)
m2       = Kv * (P1 - P2)
m3       = Kv * (P2 - P3)
out.mdot = Kv * (P3 - out.P)
der(P1)  = (in.mdot - m2) / Cc
init(P1) = P0
der(P2)  = (m2 - m3) / Cc
init(P2) = P0
der(P3)  = (m3 - out.mdot) / Cc
init(P3) = P0
T1 = Temperature(fluid$, P=P1, h=h1)
T2 = Temperature(fluid$, P=P2, h=h2)
T3 = Temperature(fluid$, P=P3, h=h3)
Q1 = UA * (w1.T - T1)
Q2 = UA * (w2.T - T2)
Q3 = UA * (w3.T - T3)
w1.Qdot = Q1
w2.Qdot = Q2
w3.Qdot = Q3
rho1 = Density(fluid$, P=P1, h=h1)
rho2 = Density(fluid$, P=P2, h=h2)
rho3 = Density(fluid$, P=P3, h=h3)
der(h1)  = (in.mdot * (in.h - h1) + Q1) / (rho1 * V)
init(h1) = h0
der(h2)  = (m2 * (h1 - h2) + Q2) / (rho2 * V)
init(h2) = h0
der(h3)  = (m3 * (h2 - h3) + Q3) / (rho3 * V)
init(h3) = h0
out.h = h3
```
