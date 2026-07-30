---
name: ContactResistance
category: Component (heat)
summary: A thermal contact resistance between two surfaces.
related: []
examples: []
tags: [contactresistance, component, heat, acausal]
---

# ContactResistance

A thermal contact resistance between two surfaces.

## Domain

A reusable **acausal heat-domain** component — its thermal ports carry temperature `T` and heat-flow rate `Q̇`; a node enforces equal `T` and `ΣQ̇ = 0`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`a`, `b`

## Usage

```
ContactResistance inst(Rth)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `Rth` | Number | Thermal resistance [K/W]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
Q      = (a.T - b.T) / Rth
a.Qdot = Q
b.Qdot = -Q
```
