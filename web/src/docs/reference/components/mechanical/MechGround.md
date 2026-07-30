---
name: MechGround
category: Component (mechanical)
summary: The rotational reference (ω = 0).
related: []
examples: []
tags: [mechground, component, mechanical, acausal]
---

# MechGround

The rotational reference (`ω = 0`).

## Domain

A reusable **acausal mechanical-domain** component — its rotational ports carry angular velocity `ω` and torque `τ` (`Στ = 0`); translational ports carry velocity `v` and force `F` (`ΣF = 0`). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`port`

## Usage

```
MechGround inst(...)
```

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
port.w = 0
```
