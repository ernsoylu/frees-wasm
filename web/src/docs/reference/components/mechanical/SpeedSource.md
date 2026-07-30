---
name: SpeedSource
category: Component (mechanical)
summary: A prescribed angular velocity.
related: []
examples: []
tags: [speedsource, component, mechanical, acausal]
---

# SpeedSource

A prescribed angular velocity.

## Domain

A reusable **acausal mechanical-domain** component — its rotational ports carry angular velocity `ω` and torque `τ` (`Στ = 0`); translational ports carry velocity `v` and force `F` (`ΣF = 0`). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`a`, `b`

## Usage

```
SpeedSource inst(w)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `w` | Number | Frequency [rad/s]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
a.w - b.w = w
a.tau + b.tau = 0
```
