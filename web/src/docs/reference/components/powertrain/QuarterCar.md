---
name: QuarterCar
category: Component (powertrain)
summary: Acausal powertrain-domain component QuarterCar with ports road.
related: []
examples: []
tags: [quartercar, component, powertrain, acausal]
references: []
generated: true
---

# QuarterCar

Reusable acausal **powertrain-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
QuarterCar inst(ms, mu, ks, cs, kt)
```

## Ports

`road`

## Parameters

| Parameter | Type |
| --- | --- |
| `ms` | Number |
| `mu` | Number |
| `ks` | Number |
| `cs` | Number |
| `kt` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
TransMass   MS(m=ms, v0=0)
TransMass   MU(m=mu, v0=0)
TransSpring KS(k=ks, x0=0)
TransDamper CS(c=cs)
TransSpring KT(k=kt, x0=0)
connect(MS.port, KS.a, CS.a)
connect(KS.b, CS.b, MU.port, KT.a)
connect(KT.b, road)
```
