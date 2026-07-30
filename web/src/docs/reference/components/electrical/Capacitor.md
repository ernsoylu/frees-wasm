---
name: Capacitor
category: Component (electrical)
summary: A capacitor storing charge, with i = C dV/dt.
related: []
examples: []
tags: [capacitor, component, electrical, acausal]
---

# Capacitor

A capacitor storing charge, with `i = C dV/dt`.

## Domain

A reusable **acausal electrical-domain** component — its electrical ports carry potential `V` and current `I`; a node enforces equal `V` and `ΣI = 0` (Kirchhoff). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`p`, `n`

## Usage

```
Capacitor inst(C, V0)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `C` | Number | Capacitance [F]. |
| `V0` | Number | Initial voltage / volume. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
Vc       = p.V - n.V
der(Vc)  = p.I / C
init(Vc) = V0
p.I + n.I = 0
```
