---
name: Supercapacitor
category: Component (electrical)
summary: Acausal electrical-domain component Supercapacitor with ports p, n.
related: []
examples: []
tags: [supercapacitor, component, electrical, acausal]
references: []
generated: true
---

# Supercapacitor

Reusable acausal **electrical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
Supercapacitor inst(C, R_esr, V0)
```

## Ports

`p`, `n`

## Parameters

| Parameter | Type |
| --- | --- |
| `C` | Number |
| `R_esr` | Number |
| `V0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(Vc)   = p.I / C
init(Vc)  = V0
p.V - n.V = Vc + R_esr * p.I
p.I + n.I = 0
```
