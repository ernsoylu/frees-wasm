---
name: TransMass
category: Component (mechanical)
summary: A translational mass, F = m dv/dt.
related: []
examples: []
tags: [transmass, component, mechanical, acausal]
---

# TransMass

A translational mass, `F = m dv/dt`.

## Domain

A reusable **acausal mechanical-domain** component — its rotational ports carry angular velocity `ω` and torque `τ` (`Στ = 0`); translational ports carry velocity `v` and force `F` (`ΣF = 0`). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`port`

## Usage

```
TransMass inst(m, v0)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `m` | Number | Mass [kg]. |
| `v0` | Number | Initial velocity [m/s]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
der(port.vel)  = port.f / m
init(port.vel) = v0
```
