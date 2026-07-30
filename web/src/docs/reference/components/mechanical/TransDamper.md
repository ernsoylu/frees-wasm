---
name: TransDamper
category: Component (mechanical)
summary: A translational viscous damper, F = c·v.
related: []
examples: []
tags: [transdamper, component, mechanical, acausal]
---

# TransDamper

A translational viscous damper, `F = c·v`.

## Domain

A reusable **acausal mechanical-domain** component — its rotational ports carry angular velocity `ω` and torque `τ` (`Στ = 0`); translational ports carry velocity `v` and force `F` (`ΣF = 0`). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`a`, `b`

## Usage

```
TransDamper inst(c)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `c` | Number | Damping / specific-heat coefficient. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
a.f = c * (a.vel - b.vel)
a.f + b.f = 0
```
