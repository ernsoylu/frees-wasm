---
name: ForceSource
category: Component (mechanical)
summary: A prescribed translational force.
related: []
examples: []
tags: [forcesource, component, mechanical, acausal]
---

# ForceSource

A prescribed translational force.

## Domain

A reusable **acausal mechanical-domain** component — its rotational ports carry angular velocity `ω` and torque `τ` (`Στ = 0`); translational ports carry velocity `v` and force `F` (`ΣF = 0`). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`a`, `b`

## Usage

```
ForceSource inst(F)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `F` | Number | Force [N]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
a.f = -F
a.f + b.f = 0
```
