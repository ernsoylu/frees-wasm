---
name: Condenser
category: Component (fluid)
summary: Rejects heat from a fluid stream to a coolant/ambient, condensing it.
related: []
examples: []
tags: [condenser, component, fluid, acausal]
---

# Condenser

Rejects heat from a fluid stream to a coolant/ambient, condensing it.

## Domain

A reusable **acausal fluid-domain** component — its thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
Condenser inst(...)
```

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.P    = in.P
Q        = in.mdot * (in.h - out.h)
```
