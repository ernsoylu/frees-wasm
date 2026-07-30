---
name: CurrentSource
category: Component (electrical)
summary: An ideal current source.
related: []
examples: []
tags: [currentsource, component, electrical, acausal]
---

# CurrentSource

An ideal current source.

## Domain

A reusable **acausal electrical-domain** component — its electrical ports carry potential `V` and current `I`; a node enforces equal `V` and `ΣI = 0` (Kirchhoff). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`p`, `n`

## Usage

```
CurrentSource inst(I)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `I` | Number | Current [A]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
p.I = -I
p.I + n.I = 0
```
