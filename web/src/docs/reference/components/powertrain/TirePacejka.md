---
name: TirePacejka
category: Component (powertrain)
summary: Acausal powertrain-domain component TirePacejka with ports wheel, veh.
related: []
examples: []
tags: [tirepacejka, component, powertrain, acausal]
references: []
generated: true
---

# TirePacejka

Reusable acausal **powertrain-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
TirePacejka inst(r, Fz, B, C, D, E, epsv)
```

## Ports

`wheel`, `veh`

## Parameters

| Parameter | Type |
| --- | --- |
| `r` | Number |
| `Fz` | Number |
| `B` | Number |
| `C` | Number |
| `D` | Number |
| `E` | Number |
| `epsv` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
v_w       = r * wheel.w
slip      = (v_w - veh.vel) / (abs(veh.vel) + epsv)
Bs        = B * slip
Fx        = Fz * D * sin(C * arctan(Bs - E * (Bs - arctan(Bs))))
veh.f     = -Fx
wheel.tau = r * Fx
```
