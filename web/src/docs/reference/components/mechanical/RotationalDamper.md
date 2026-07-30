---
name: RotationalDamper
category: Component (mechanical)
summary: A rotational viscous damper, τ = c·ω.
related: []
examples: []
tags: [rotationaldamper, component, mechanical, acausal]
---

# RotationalDamper

A rotational viscous damper, `τ = c·ω`.

## Domain

A reusable **acausal mechanical-domain** component — its rotational ports carry angular velocity `ω` and torque `τ` (`Στ = 0`); translational ports carry velocity `v` and force `F` (`ΣF = 0`). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`a`, `b`

## Usage

```
RotationalDamper inst(c)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `c` | Number | Damping / specific-heat coefficient. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
a.tau = c * (a.w - b.w)
a.tau + b.tau = 0
```
