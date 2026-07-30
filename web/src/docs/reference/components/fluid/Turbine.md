---
name: Turbine
category: Component (fluid)
summary: Extracts work from an expanding fluid stream, computing it from an isentropic efficiency.
related: []
examples: []
tags: [turbine, component, fluid, acausal]
---

# Turbine

Extracts work from an expanding fluid stream, computing it from an isentropic efficiency.

## Domain

A reusable **acausal fluid-domain** component — its thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
Turbine inst(eta, fluid$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `eta` | Number | Efficiency (0–1). |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
s_in     = Entropy(fluid$, P=in.P, h=in.h)
h_s      = Enthalpy(fluid$, P=out.P, s=s_in)
out.mdot = in.mdot
out.h    = in.h - eta * (in.h - h_s)
W        = in.mdot * (in.h - out.h)
```
