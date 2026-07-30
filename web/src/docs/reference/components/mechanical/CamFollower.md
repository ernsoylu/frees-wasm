---
name: CamFollower
category: Component (mechanical)
summary: Acausal mechanical-domain component CamFollower with ports rod.
related: []
examples: []
tags: [camfollower, component, mechanical, acausal]
references: []
generated: true
---

# CamFollower

Reusable acausal **mechanical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
CamFollower inst(m, kspring, x0, v0)
```

## Ports

`rod`

## Parameters

| Parameter | Type |
| --- | --- |
| `m` | Number |
| `kspring` | Number |
| `x0` | Number |
| `v0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
TransMass   M(m=m, v0=v0)
TransSpring S(k=kspring, x0=x0)
TransGround G()
connect(rod, M.port, S.a)
connect(S.b, G.port)
```
