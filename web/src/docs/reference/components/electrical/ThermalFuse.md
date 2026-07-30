---
name: ThermalFuse
category: Component (electrical)
summary: Acausal electrical-domain component ThermalFuse with ports p, n.
related: []
examples: []
tags: [thermalfuse, component, electrical, acausal]
references: []
generated: true
---

# ThermalFuse

Reusable acausal **electrical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
ThermalFuse inst(R0, Iblow, kR, epsI)
```

## Ports

`p`, `n`

## Parameters

| Parameter | Type |
| --- | --- |
| `R0` | Number |
| `Iblow` | Number |
| `kR` | Number |
| `epsI` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
m         = 1 + kR * 0.5 * (1 + tanh((abs(p.I) - Iblow) / epsI))
p.V - n.V = R0 * m * p.I
p.I + n.I = 0
```
