---
name: TwoZoneHX
category: Component (fluid)
summary: A two-zone heat exchanger resolving distinct thermal regions.
related: []
examples: []
tags: [twozonehx, component, fluid, acausal]
---

# TwoZoneHX

A two-zone heat exchanger resolving distinct thermal regions.

## Domain

A reusable **acausal fluid-domain** component — its thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`hot_in`, `hot_out`, `cold_in`, `cold_out`

## Usage

```
TwoZoneHX inst(UA, hot$, cold$, arr$)
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
HeatExchanger C1(UA=UA/2, hot$=hot$, cold$=cold$, arr$=arr$)
HeatExchanger C2(UA=UA/2, hot$=hot$, cold$=cold$, arr$=arr$)
connect(hot_in, C1.hot_in)
connect(C1.hot_out, C2.hot_in)
connect(C2.hot_out, hot_out)
connect(cold_in, C2.cold_in)
connect(C2.cold_out, C1.cold_in)
connect(C1.cold_out, cold_out)
```
