---
name: Sink
category: Component (fluid)
summary: A fluid boundary that absorbs a stream at a set pressure.
related: []
examples: []
tags: [sink, component, fluid, acausal]
---

# Sink

A fluid boundary that absorbs a stream at a set pressure.

## Domain

A reusable **acausal fluid-domain** component — its thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`

## Usage

```
Sink inst(...)
```

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
mdot = in.mdot
P    = in.P
h    = in.h
```
