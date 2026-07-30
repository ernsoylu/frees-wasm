---
name: ThermalSwitch
category: Component (heat)
summary: Acausal heat-domain component ThermalSwitch with ports a, b.
related: []
examples: []
tags: [thermalswitch, component, heat, acausal]
references: []
generated: true
---

# ThermalSwitch

Reusable acausal **heat-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
ThermalSwitch inst(G, Ton, band)
```

## Ports

`a`, `b`

## Parameters

| Parameter | Type |
| --- | --- |
| `G` | Number |
| `Ton` | Number |
| `band` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
u      = 0.5 * (1 + tanh((a.T - Ton) / band))
a.Qdot = u * G * (a.T - b.T)
a.Qdot + b.Qdot = 0
```
