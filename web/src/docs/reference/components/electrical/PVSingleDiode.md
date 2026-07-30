---
name: PVSingleDiode
category: Component (electrical)
summary: Acausal electrical-domain component PVSingleDiode with ports p, n, G.
related: []
examples: []
tags: [pvsinglediode, component, electrical, acausal]
references: []
generated: true
---

# PVSingleDiode

Reusable acausal **electrical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
PVSingleDiode inst(Isc_ref, Gref, I0d, n_d, Vt, Rs, Rsh)
```

## Ports

`p`, `n`, `G`

## Parameters

| Parameter | Type |
| --- | --- |
| `Isc_ref` | Number |
| `Gref` | Number |
| `I0d` | Number |
| `n_d` | Number |
| `Vt` | Number |
| `Rs` | Number |
| `Rsh` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
V   = p.V - n.V
I   = -p.I
Iph = Isc_ref * G.sig / Gref
Vd  = V + I * Rs
I   = Iph - I0d * (exp(Vd / (n_d * Vt)) - 1) - Vd / Rsh
p.I + n.I = 0
```
