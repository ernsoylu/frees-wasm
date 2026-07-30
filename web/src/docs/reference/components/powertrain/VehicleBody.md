---
name: VehicleBody
category: Component (powertrain)
summary: Acausal powertrain-domain component VehicleBody with ports port.
related: []
examples: []
tags: [vehiclebody, component, powertrain, acausal]
references: []
generated: true
---

# VehicleBody

Reusable acausal **powertrain-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
VehicleBody inst(m, Cd, Af, rhoA, Crr, grade, v0)
```

## Ports

`port`

## Parameters

| Parameter | Type |
| --- | --- |
| `m` | Number |
| `Cd` | Number |
| `Af` | Number |
| `rhoA` | Number |
| `Crr` | Number |
| `grade` | Number |
| `v0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
F_res = 0.5 * rhoA * Cd * Af * port.vel * abs(port.vel) + m * 9.80665 * (Crr * tanh(port.vel / 0.1) + sin(grade))
der(port.vel)  = (port.f - F_res) / m
init(port.vel) = v0
```
