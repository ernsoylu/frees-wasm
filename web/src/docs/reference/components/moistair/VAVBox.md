---
name: VAVBox
category: Component (moistair)
summary: Acausal moistair-domain component VAVBox with ports in, out, u, ur.
related: []
examples: []
tags: [vavbox, component, moistair, acausal]
references: []
generated: true
---

# VAVBox

Reusable acausal **moistair-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
VAVBox inst(mdot_max, Qr_max, domain$)
```

## Ports

`in`, `out`, `u`, `ur`

## Parameters

| Parameter | Type |
| --- | --- |
| `mdot_max` | Number |
| `Qr_max` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
in.mdot  = u.sig * mdot_max
out.mdot = in.mdot
out.W    = in.W
out.h    = in.h + ur.sig * Qr_max / in.mdot
```
