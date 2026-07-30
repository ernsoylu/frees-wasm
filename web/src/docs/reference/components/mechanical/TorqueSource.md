---
name: TorqueSource
category: Component (mechanical)
summary: A prescribed torque.
related: []
examples: []
tags: [torquesource, component, mechanical, acausal]
---

# TorqueSource

A prescribed torque.

## Domain

A reusable **acausal mechanical-domain** component — its rotational ports carry angular velocity `ω` and torque `τ` (`Στ = 0`); translational ports carry velocity `v` and force `F` (`ΣF = 0`). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`a`, `b`

## Usage

```
TorqueSource inst(T)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `T` | Number | Temperature [K]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
a.tau = -T
a.tau + b.tau = 0
```
