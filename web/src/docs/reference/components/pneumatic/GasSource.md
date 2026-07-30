---
name: GasSource
category: Component (pneumatic)
summary: A boundary supplying gas at set conditions.
related: []
examples: []
tags: [gassource, component, pneumatic, acausal]
references:
  - "ISO 6358 — Pneumatic fluid power: flow-rate characteristics"
---

# GasSource

A boundary supplying gas at set conditions.

## Domain

A reusable **acausal pneumatic-domain** component — its compressible-gas ports carry pressure `P`, mass-flow `ṁ`, and enthalpy `h` (ISO 6358 flow). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`out`

## Usage

```
GasSource inst(y, mdot, P, h0)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `y` | Number | Position / fraction. |
| `mdot` | Number | Mass flow rate [kg/s]. |
| `P` | Number | Pressure [Pa]. |
| `h0` | Number | Reference enthalpy [J/kg]. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.y    = y
out.mdot = mdot
out.P    = P
out.h    = h0
```

## References

1. ISO 6358 — Pneumatic fluid power: flow-rate characteristics.
