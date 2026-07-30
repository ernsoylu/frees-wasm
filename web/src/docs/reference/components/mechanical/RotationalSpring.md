---
name: RotationalSpring
category: Component (mechanical)
summary: A torsional spring, τ = k·θ.
related: []
examples: []
tags: [rotationalspring, component, mechanical, acausal]
---

# RotationalSpring

A torsional spring, `τ = k·θ`.

## Domain

A reusable **acausal mechanical-domain** component — its rotational ports carry angular velocity `ω` and torque `τ` (`Στ = 0`); translational ports carry velocity `v` and force `F` (`ΣF = 0`). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`a`, `b`

## Usage

```
RotationalSpring inst(k, theta0)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `k` | Number | Stiffness / conductivity. |
| `theta0` | Number | Initial angle [rad]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
der(theta)  = a.w - b.w
init(theta) = theta0
a.tau       = k * theta
a.tau + b.tau = 0
```
