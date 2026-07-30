---
name: Friction
category: Component (mechanical)
summary: A friction element opposing motion.
related: []
examples: []
tags: [friction, component, mechanical, acausal]
---

# Friction

A friction element opposing motion.

## Domain

A reusable **acausal mechanical-domain** component — its rotational ports carry angular velocity `ω` and torque `τ` (`Στ = 0`); translational ports carry velocity `v` and force `F` (`ΣF = 0`). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`a`, `b`

## Usage

```
Friction inst(Fc, Fs, vs, bv, eps)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `Fc` | Number | Coulomb friction force [N]. |
| `Fs` | Number | Static friction force [N]. |
| `vs` | Number | Reference / slip velocity [m/s]. |
| `bv` | Number | Viscous-friction coefficient. |
| `eps` | Number | Effectiveness / roughness. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
dw    = a.w - b.w
a.tau = (Fc + (Fs - Fc) * exp(-(dw / vs)^2)) * tanh(dw / eps) + bv * dw
a.tau + b.tau = 0
```
