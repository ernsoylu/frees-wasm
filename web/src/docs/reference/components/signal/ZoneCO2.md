---
name: ZoneCO2
category: Component (signal)
summary: Acausal signal-domain component ZoneCO2 with ports vent, occ, out.
related: []
examples: []
tags: [zoneco2, component, signal, acausal]
references: []
generated: true
---

# ZoneCO2

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
ZoneCO2 inst(Vz, c_amb, gen_occ, c0)
```

## Ports

`vent`, `occ`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `Vz` | Number |
| `c_amb` | Number |
| `gen_occ` | Number |
| `c0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(c)  = (vent.sig * (c_amb - c) + occ.sig * gen_occ) / Vz
init(c) = c0
out.sig = c
```
