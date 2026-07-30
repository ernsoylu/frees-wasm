---
name: InverterLoss
category: Component (electrical)
summary: Acausal electrical-domain component InverterLoss with ports in_p, out_p, heat.
related: []
examples: []
tags: [inverterloss, component, electrical, acausal]
references: []
generated: true
---

# InverterLoss

Reusable acausal **electrical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
InverterLoss inst(V0, r, Esw, fsw, Iref, Vref, Vnom, epsI)
```

## Ports

`in_p`, `out_p`, `heat`

## Parameters

| Parameter | Type |
| --- | --- |
| `V0` | Number |
| `r` | Number |
| `Esw` | Number |
| `fsw` | Number |
| `Iref` | Number |
| `Vref` | Number |
| `Vnom` | Number |
| `epsI` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
I         = in_p.I
in_p.I + out_p.I = 0
Vsw       = Esw * fsw * Vnom / (Iref * Vref)
dV        = (V0 + Vsw) * tanh(I / epsI) + r * I
out_p.V   = in_p.V - dV
Q         = dV * I
heat.Qdot = -Q
```
