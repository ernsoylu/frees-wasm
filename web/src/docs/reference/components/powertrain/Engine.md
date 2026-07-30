---
name: Engine
category: Component (powertrain)
summary: An internal-combustion engine acting as a torque source.
related: []
examples: [engine-map-2d, engine-cycle-wiebe]
tags: [engine, component, powertrain, acausal]
---

# Engine

An internal-combustion engine acting as a torque source.

## Domain

A reusable **acausal powertrain-domain** component — its rotational ports carry angular velocity `ω` and torque `τ`, with vehicle-level speed/force signals. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`shaft`

## Usage

```
Engine inst(Tmax, throttle, bf)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `Tmax` | Number | Maximum temperature [K]. |
| `throttle` | Number | Throttle (0–1). |
| `bf` | Number | Friction coefficient. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
shaft.tau = -(throttle * Tmax - bf * shaft.w)
```

## Examples

Instantiated in the verified example below:

[Run: engine-map-2d]
