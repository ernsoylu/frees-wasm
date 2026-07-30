---
name: TwoPhaseEjector
category: Component (twophase)
summary: Acausal twophase-domain component TwoPhaseEjector with ports m, s, out.
related: []
examples: []
tags: [twophaseejector, component, twophase, acausal]
references: []
generated: true
---

# TwoPhaseEjector

Reusable acausal **twophase-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
TwoPhaseEjector inst(PLR, domain$)
```

## Ports

`m`, `s`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `PLR` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = m.mdot + s.mdot
out.mdot * out.h = m.mdot * m.h + s.mdot * s.h
out.P = PLR * s.P
```
