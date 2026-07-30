---
name: VoltageSource
category: Component (electrical)
summary: An ideal voltage source.
related: []
examples: [pressure-cooker]
tags: [voltagesource, component, electrical, acausal]
---

# VoltageSource

An ideal voltage source.

## Domain

A reusable **acausal electrical-domain** component — its electrical ports carry potential `V` and current `I`; a node enforces equal `V` and `ΣI = 0` (Kirchhoff). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`p`, `n`

## Usage

```
VoltageSource inst(E)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `E` | Number | EMF / voltage [V]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
p.V - n.V = E
p.I + n.I = 0
```

## Examples

Instantiated in the verified example below:

[Run: pressure-cooker]
