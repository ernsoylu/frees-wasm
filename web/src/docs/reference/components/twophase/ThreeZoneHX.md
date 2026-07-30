---
name: ThreeZoneHX
category: Component (twophase)
summary: A three-zone (subcooled / two-phase / superheat) heat exchanger.
related: []
examples: []
tags: [threezonehx, component, twophase, acausal]
---

# ThreeZoneHX

A three-zone (subcooled / two-phase / superheat) heat exchanger.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`hot_in`, `hot_out`, `cold_in`, `cold_out`

## Usage

```
ThreeZoneHX inst(UA, hot$, cold$, arr$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `UA` | Number | Overall conductance UA [W/K]. |
| `hot$` | String | Hot-side fluid name (e.g. Water). |
| `cold$` | String | Cold-side fluid name (e.g. EG50). |
| `arr$` | String | Flow arrangement (passed to hx_effectiveness) — one of `counterflow`, `parallel`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
HeatExchanger Z1(UA=UA/3, hot$=hot$, cold$=cold$, arr$=arr$)
HeatExchanger Z2(UA=UA/3, hot$=hot$, cold$=cold$, arr$=arr$)
HeatExchanger Z3(UA=UA/3, hot$=hot$, cold$=cold$, arr$=arr$)
connect(hot_in, Z1.hot_in)
connect(Z1.hot_out, Z2.hot_in)
connect(Z2.hot_out, Z3.hot_in)
connect(Z3.hot_out, hot_out)
connect(cold_in, Z3.cold_in)
connect(Z3.cold_out, Z2.cold_in)
connect(Z2.cold_out, Z1.cold_in)
connect(Z1.cold_out, cold_out)
```
