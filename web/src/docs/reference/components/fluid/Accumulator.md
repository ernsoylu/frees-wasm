---
name: Accumulator
category: Component (fluid)
summary: A fluid accumulator — a compliance volume that stores fluid under pressure and buffers flow transients.
related: []
examples: []
tags: [accumulator, component, fluid, acausal]
---

# Accumulator

A fluid accumulator — a compliance volume that stores fluid under pressure and buffers flow transients.

## Domain

A reusable **acausal fluid-domain** component — its thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
Accumulator inst(C, P0)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `C` | Number | Capacitance [F]. |
| `P0` | Number | Reference/initial pressure [Pa]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.P       = in.P
out.h       = in.h
der(in.P)   = (in.mdot - out.mdot) / C
init(in.P)  = P0
```
