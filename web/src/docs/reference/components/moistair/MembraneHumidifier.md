---
name: MembraneHumidifier
category: Component (moistair)
summary: Acausal moistair-domain component MembraneHumidifier with ports dry_in, dry_out, wet_in, wet_out.
related: []
examples: []
tags: [membranehumidifier, component, moistair, acausal]
references: []
generated: true
---

# MembraneHumidifier

Reusable acausal **moistair-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
MembraneHumidifier inst(eff_h, eff_w, domain$)
```

## Ports

`dry_in`, `dry_out`, `wet_in`, `wet_out`

## Parameters

| Parameter | Type |
| --- | --- |
| `eff_h` | Number |
| `eff_w` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
dry_out.mdot = dry_in.mdot
wet_out.mdot = wet_in.mdot
dry_out.P    = dry_in.P
wet_out.P    = wet_in.P
dry_out.W    = dry_in.W + eff_w * (wet_in.W - dry_in.W)
dry_out.h    = dry_in.h + eff_h * (wet_in.h - dry_in.h)
wet_out.W    = wet_in.W - (dry_in.mdot / wet_in.mdot) * (dry_out.W - dry_in.W)
wet_out.h    = wet_in.h - (dry_in.mdot / wet_in.mdot) * (dry_out.h - dry_in.h)
```
