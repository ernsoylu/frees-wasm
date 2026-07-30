---
name: Transmission
category: Component (powertrain)
summary: A gearbox/transmission imposing a ratio between engine and wheels.
related: []
examples: []
tags: [transmission, component, powertrain, acausal]
---

# Transmission

A gearbox/transmission imposing a ratio between engine and wheels.

## Domain

A reusable **acausal powertrain-domain** component — its rotational ports carry angular velocity `ω` and torque `τ`, with vehicle-level speed/force signals. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
Transmission inst(ratio, eta)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `ratio` | Number | Gear / split ratio. |
| `eta` | Number | Efficiency (0–1). |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
in.w    = ratio * out.w
out.tau = -ratio * eta * in.tau
```
