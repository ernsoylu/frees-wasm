---
name: Inductor
category: Component (electrical)
summary: An inductor storing magnetic energy, with V = L di/dt.
related: []
examples: []
tags: [inductor, component, electrical, acausal]
---

# Inductor

An inductor storing magnetic energy, with `V = L di/dt`.

## Domain

A reusable **acausal electrical-domain** component — its electrical ports carry potential `V` and current `I`; a node enforces equal `V` and `ΣI = 0` (Kirchhoff). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`p`, `n`

## Usage

```
Inductor inst(L, I0)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `L` | Number | Length [m]. |
| `I0` | Number | Saturation current [A]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
der(IL)  = (p.V - n.V) / L
init(IL) = I0
p.I = IL
p.I + n.I = 0
```
