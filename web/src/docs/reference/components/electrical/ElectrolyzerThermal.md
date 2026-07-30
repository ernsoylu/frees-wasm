---
name: ElectrolyzerThermal
category: Component (electrical)
summary: Acausal electrical-domain component ElectrolyzerThermal with ports p, n, cool_in, cool_out.
related: []
examples: []
tags: [electrolyzerthermal, component, electrical, acausal]
references: []
generated: true
---

# ElectrolyzerThermal

Reusable acausal **electrical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
ElectrolyzerThermal inst(ncells, area, i0, Rohm, E0, alpha, Eth, T, fluid$, UA)
```

## Ports

`p`, `n`, `cool_in`, `cool_out`

## Parameters

| Parameter | Type |
| --- | --- |
| `ncells` | Number |
| `area` | Number |
| `i0` | Number |
| `Rohm` | Number |
| `E0` | Number |
| `alpha` | Number |
| `Eth` | Number |
| `T` | Number |
| `fluid$` | String |
| `UA` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
Electrolyzer EL(ncells=ncells, area=area, i0=i0, Rohm=Rohm, E0=E0, alpha=alpha, Eth=Eth, T=T)
LiquidWallHX HX(fluid$=fluid$, UA=UA)
connect(p, EL.p)
connect(n, EL.n)
connect(EL.heat, HX.wall)
connect(cool_in, HX.in)
connect(HX.out, cool_out)
```
