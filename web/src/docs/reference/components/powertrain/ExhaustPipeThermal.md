---
name: ExhaustPipeThermal
category: Component (powertrain)
summary: Acausal powertrain-domain component ExhaustPipeThermal with ports in, out, amb.
related: []
examples: []
tags: [exhaustpipethermal, component, powertrain, acausal]
references: []
generated: true
---

# ExhaustPipeThermal

Reusable acausal **powertrain-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
ExhaustPipeThermal inst(fluid$, UA, hA, C1, C2, R, T10, T20)
```

## Ports

`in`, `out`, `amb`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `UA` | Number |
| `hA` | Number |
| `C1` | Number |
| `C2` | Number |
| `R` | Number |
| `T10` | Number |
| `T20` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
HeatedDuct D(fluid$=fluid$, UA=UA)
WallRC     W(C1=C1, C2=C2, R=R, T10=T10, T20=T20)
Convection CV(htc=hA, area=1)
connect(in, D.in)
connect(D.out, out)
connect(D.wall, W.a)
connect(W.b, CV.a)
connect(CV.b, amb)
```
