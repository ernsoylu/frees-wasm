---
name: Clutch
category: Component (mechanical)
summary: A friction clutch coupling/decoupling two rotational shafts.
related: []
examples: []
tags: [clutch, component, mechanical, acausal]
---

# Clutch

A friction clutch coupling/decoupling two rotational shafts.

## Domain

A reusable **acausal mechanical-domain** component — its rotational ports carry angular velocity `ω` and torque `τ` (`Στ = 0`); translational ports carry velocity `v` and force `F` (`ΣF = 0`). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`a`, `b`

## Usage

```
Clutch inst(Tmax, eng, eps)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `Tmax` | Number | Maximum temperature [K]. |
| `eng` | Number | Engagement fraction (0–1). |
| `eps` | Number | Effectiveness / roughness. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
dw    = a.w - b.w
a.tau = eng * Tmax * tanh(dw / eps)
a.tau + b.tau = 0
```
