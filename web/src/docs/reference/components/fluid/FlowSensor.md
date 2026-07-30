---
name: FlowSensor
category: Component (fluid)
summary: Measures the mass flow of a stream (a pass-through sensor).
related: []
examples: []
tags: [flowsensor, component, fluid, acausal]
---

# FlowSensor

Measures the mass flow of a stream (a pass-through sensor).

## Domain

A reusable **acausal fluid-domain** component — its thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
FlowSensor inst(...)
```

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot  = in.mdot
out.P     = in.P
out.h     = in.h
mdot_meas = in.mdot
```
