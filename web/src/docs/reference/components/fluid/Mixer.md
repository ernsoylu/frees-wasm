---
name: Mixer
category: Component (fluid)
summary: Combines two fluid streams into one, with flow-weighted enthalpy mixing.
related: []
examples: []
tags: [mixer, component, fluid, acausal]
---

# Mixer

Combines two fluid streams into one, with flow-weighted enthalpy mixing.

## Domain

A reusable **acausal fluid-domain** component — its thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in1`, `in2`, `out`

## Usage

```
Mixer inst(...)
```

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.P    = in1.P
out.mdot = in1.mdot + in2.mdot
out.mdot * out.h = in1.mdot * in1.h + in2.mdot * in2.h
```
