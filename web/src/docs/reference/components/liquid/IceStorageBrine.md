---
name: IceStorageBrine
category: Component (liquid)
summary: Acausal liquid-domain component IceStorageBrine with ports in, out.
related: []
examples: []
tags: [icestoragebrine, component, liquid, acausal]
references: []
generated: true
---

# IceStorageBrine

Reusable acausal **liquid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
IceStorageBrine inst(fluid$, UA, m, cp_p, L, Tm, dTm, T0)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `UA` | Number |
| `m` | Number |
| `cp_p` | Number |
| `L` | Number |
| `Tm` | Number |
| `dTm` | Number |
| `T0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
LiquidWallHX HX(fluid$=fluid$, UA=UA)
PCMMass      ICE(m=m, cp=cp_p, L=L, Tm=Tm, dTm=dTm, T0=T0)
connect(in, HX.in)
connect(HX.out, out)
connect(HX.wall, ICE.port)
```
