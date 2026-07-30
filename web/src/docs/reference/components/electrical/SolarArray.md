---
name: SolarArray
category: Component (electrical)
summary: Acausal electrical-domain component SolarArray with ports p, n, G.
related: []
examples: []
tags: [solararray, component, electrical, acausal]
references: []
generated: true
---

# SolarArray

Reusable acausal **electrical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SolarArray inst(Isc_ref, Gref, Voc, epsV)
```

## Ports

`p`, `n`, `G`

## Parameters

| Parameter | Type |
| --- | --- |
| `Isc_ref` | Number |
| `Gref` | Number |
| `Voc` | Number |
| `epsV` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
Iph = Isc_ref * G.sig / Gref
V   = p.V - n.V
p.I = -Iph * 0.5 * (1 + tanh((Voc - V) / epsV))
p.I + n.I = 0
```
