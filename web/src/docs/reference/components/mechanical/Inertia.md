---
name: Inertia
category: Component (mechanical)
summary: A rotational inertia, τ = J dω/dt.
related: []
examples: []
tags: [inertia, component, mechanical, acausal]
---

# Inertia

A rotational inertia, `τ = J dω/dt`.

## Domain

A reusable **acausal mechanical-domain** component — its rotational ports carry angular velocity `ω` and torque `τ` (`Στ = 0`); translational ports carry velocity `v` and force `F` (`ΣF = 0`). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`port`

## Usage

```
Inertia inst(J, w0)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `J` | Number | Inertia [kg·m²]. |
| `w0` | Number | Natural frequency [rad/s]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
der(port.w)  = port.tau / J
init(port.w) = w0
```
