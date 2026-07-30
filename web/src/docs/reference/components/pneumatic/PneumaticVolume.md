---
name: PneumaticVolume
category: Component (pneumatic)
summary: A pneumatic control volume (compressible capacitance).
related: []
examples: []
tags: [pneumaticvolume, component, pneumatic, acausal]
references:
  - "ISO 6358 — Pneumatic fluid power: flow-rate characteristics"
---

# PneumaticVolume

A pneumatic control volume (compressible capacitance).

## Domain

A reusable **acausal pneumatic-domain** component — its compressible-gas ports carry pressure `P`, mass-flow `ṁ`, and enthalpy `h` (ISO 6358 flow). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
PneumaticVolume inst(V, T, R, P0, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `V` | Number | Volume [m³]. |
| `T` | Number | Temperature [K]. |
| `R` | Number | Resistance [Ω]. |
| `P0` | Number | Reference/initial pressure [Pa]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.P      = in.P
out.h      = in.h
der(in.P)  = (R * T / V) * (in.mdot - out.mdot)
init(in.P) = P0
```

## References

1. ISO 6358 — Pneumatic fluid power: flow-rate characteristics.
