---
name: Gear
category: Component (mechanical)
summary: A gear pair imposing a fixed speed/torque ratio between two shafts.
related: []
examples: []
tags: [gear, component, mechanical, acausal]
---

# Gear

A gear pair imposing a fixed speed/torque ratio between two shafts.

## Domain

A reusable **acausal mechanical-domain** component — its rotational ports carry angular velocity `ω` and torque `τ` (`Στ = 0`); translational ports carry velocity `v` and force `F` (`ΣF = 0`). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
Gear inst(ratio)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `ratio` | Number | Gear / split ratio. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
in.w    = ratio * out.w
out.tau = -ratio * in.tau
```
