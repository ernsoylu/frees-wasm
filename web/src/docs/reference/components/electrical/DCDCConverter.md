---
name: DCDCConverter
category: Component (electrical)
summary: Acausal electrical-domain component DCDCConverter with ports in_p, in_n, out_p, out_n.
related: []
examples: []
tags: [dcdcconverter, component, electrical, acausal]
references: []
generated: true
---

# DCDCConverter

Reusable acausal **electrical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
DCDCConverter inst(ratio, eta, epsP)
```

## Ports

`in_p`, `in_n`, `out_p`, `out_n`

## Parameters

| Parameter | Type |
| --- | --- |
| `ratio` | Number |
| `eta` | Number |
| `epsP` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out_p.V - out_n.V = ratio * (in_p.V - in_n.V)
in_p.I + in_n.I   = 0
out_p.I + out_n.I = 0
Pout = (out_p.V - out_n.V) * (0 - out_p.I)
s    = 0.5 * (1 + tanh(Pout / epsP))
Pin  = s * Pout / eta + (1 - s) * Pout * eta
Pin  = (in_p.V - in_n.V) * in_p.I
```
