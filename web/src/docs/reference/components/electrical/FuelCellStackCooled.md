---
name: FuelCellStackCooled
category: Component (electrical)
summary: Acausal electrical-domain component FuelCellStackCooled with ports p, n, cool_in, cool_out.
related: []
examples: []
tags: [fuelcellstackcooled, component, electrical, acausal]
references: []
generated: true
---

# FuelCellStackCooled

Reusable acausal **electrical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
FuelCellStackCooled inst(ncells, area, i0, ilim, Rohm, E0, alpha, Eth, T, fluid$, UA)
```

## Ports

`p`, `n`, `cool_in`, `cool_out`

## Parameters

| Parameter | Type |
| --- | --- |
| `ncells` | Number |
| `area` | Number |
| `i0` | Number |
| `ilim` | Number |
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
FuelCellStack FC(ncells=ncells, area=area, i0=i0, ilim=ilim, Rohm=Rohm, E0=E0, alpha=alpha, Eth=Eth, T=T)
LiquidWallHX  HX(fluid$=fluid$, UA=UA)
connect(p, FC.p)
connect(n, FC.n)
connect(FC.heat, HX.wall)
connect(cool_in, HX.in)
connect(HX.out, cool_out)
```
