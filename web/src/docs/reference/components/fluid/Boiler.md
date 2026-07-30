---
name: Boiler
category: Component (fluid)
summary: Adds heat to a fluid stream, raising its enthalpy (and generating vapor at saturation).
related: []
examples: []
tags: [boiler, component, fluid, acausal]
---

# Boiler

Adds heat to a fluid stream, raising its enthalpy (and generating vapor at saturation).

## Domain

A reusable **acausal fluid-domain** component — its thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
Boiler inst(...)
```

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.P    = in.P
Q        = in.mdot * (out.h - in.h)
```
