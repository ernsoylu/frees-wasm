---
name: GearboxScheduled
category: Component (powertrain)
summary: Acausal powertrain-domain component GearboxScheduled with ports in, out, u.
related: []
examples: []
tags: [gearboxscheduled, component, powertrain, acausal]
references: []
generated: true
---

# GearboxScheduled

Reusable acausal **powertrain-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
GearboxScheduled inst(eta)
```

## Ports

`in`, `out`, `u`

## Parameters

| Parameter | Type |
| --- | --- |
| `eta` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
in.w    = u.sig * out.w
out.tau = -u.sig * eta * in.tau
```
