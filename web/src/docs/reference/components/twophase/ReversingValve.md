---
name: ReversingValve
category: Component (twophase)
summary: Acausal twophase-domain component ReversingValve with ports d, s, i, o.
related: []
examples: []
tags: [reversingvalve, component, twophase, acausal]
references: []
generated: true
---

# ReversingValve

Reusable acausal **twophase-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
ReversingValve inst(mode, domain$)
```

## Ports

`d`, `s`, `i`, `o`

## Parameters

| Parameter | Type |
| --- | --- |
| `mode` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
o.P    = (1 - mode) * d.P + mode * s.P
i.P    = (1 - mode) * s.P + mode * d.P
o.mdot = (1 - mode) * d.mdot + mode * s.mdot
i.mdot = (1 - mode) * s.mdot + mode * d.mdot
(1 - mode) * (o.h - d.h) + mode * (s.h - o.h) = 0
(1 - mode) * (s.h - i.h) + mode * (i.h - d.h) = 0
```
