---
name: MeanValueEngine
category: Component (powertrain)
summary: A mean-value engine model (cycle-averaged torque and flows).
related: []
examples: []
tags: [meanvalueengine, component, powertrain, acausal]
---

# MeanValueEngine

A mean-value engine model (cycle-averaged torque and flows).

## Domain

A reusable **acausal powertrain-domain** component — its rotational ports carry angular velocity `ω` and torque `τ`, with vehicle-level speed/force signals. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`shaft`

## Usage

```
MeanValueEngine inst(throttle, Tpeak, w_peak, FMEP_a, FMEP_b)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `throttle` | Number | Throttle (0–1). |
| `Tpeak` | Number | Peak temperature [K]. |
| `w_peak` | Number | Peak frequency [rad/s]. |
| `FMEP_a` | Number | Friction-MEP constant [Pa]. |
| `FMEP_b` | Number | Friction-MEP slope coefficient. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
T_wot     = Tpeak * (1 - ((shaft.w - w_peak) / w_peak)^2)
T_ind     = throttle * T_wot
T_fric    = FMEP_a + FMEP_b * shaft.w
shaft.tau = -(T_ind - T_fric)
```
