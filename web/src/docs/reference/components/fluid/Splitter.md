---
name: Splitter
category: Component (fluid)
summary: Divides a fluid stream into two branches.
related: []
examples: []
tags: [splitter, component, fluid, acausal]
---

# Splitter

Divides a fluid stream into two branches.

## Domain

A reusable **acausal fluid-domain** component — its thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out1`, `out2`

## Usage

```
Splitter inst(...)
```

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out1.P   = in.P
out2.P   = in.P
out1.h   = in.h
out2.h   = in.h
in.mdot  = out1.mdot + out2.mdot
```
