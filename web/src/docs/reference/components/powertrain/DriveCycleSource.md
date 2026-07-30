---
name: DriveCycleSource
category: Component (powertrain)
summary: Acausal powertrain-domain component DriveCycleSource with ports port.
related: []
examples: []
tags: [drivecyclesource, component, powertrain, acausal]
references: []
generated: true
---

# DriveCycleSource

Reusable acausal **powertrain-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
DriveCycleSource inst(map$)
```

## Ports

`port`

## Parameters

| Parameter | Type |
| --- | --- |
| `map$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
port.vel = map$(time)
```
