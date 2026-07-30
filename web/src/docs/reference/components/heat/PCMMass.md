---
name: PCMMass
category: Component (heat)
summary: Acausal heat-domain component PCMMass with ports port.
related: []
examples: []
tags: [pcmmass, component, heat, acausal]
references: []
generated: true
---

# PCMMass

Reusable acausal **heat-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
PCMMass inst(m, cp, L, Tm, dTm, T0)
```

## Ports

`port`

## Parameters

| Parameter | Type |
| --- | --- |
| `m` | Number |
| `cp` | Number |
| `L` | Number |
| `Tm` | Number |
| `dTm` | Number |
| `T0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
cpe          = cp + (L / (dTm * 1.7724539)) * exp(-((port.T - Tm) / dTm)^2)
der(port.T)  = port.Qdot / (m * cpe)
init(port.T) = T0
```
