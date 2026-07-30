---
name: ChargerCCCV
category: Component (electrical)
summary: Acausal electrical-domain component ChargerCCCV with ports p, n.
related: []
examples: []
tags: [chargercccv, component, electrical, acausal]
references: []
generated: true
---

# ChargerCCCV

Reusable acausal **electrical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
ChargerCCCV inst(Imax, Vmax, epsV)
```

## Ports

`p`, `n`

## Parameters

| Parameter | Type |
| --- | --- |
| `Imax` | Number |
| `Vmax` | Number |
| `epsV` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
V   = p.V - n.V
p.I = -Imax * 0.5 * (1 + tanh((Vmax - V) / epsV))
p.I + n.I = 0
```
