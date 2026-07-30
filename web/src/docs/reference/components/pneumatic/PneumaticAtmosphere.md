---
name: PneumaticAtmosphere
category: Component (pneumatic)
summary: An atmospheric (ambient-pressure) pneumatic boundary.
related: []
examples: []
tags: [pneumaticatmosphere, component, pneumatic, acausal]
references:
  - "ISO 6358 — Pneumatic fluid power: flow-rate characteristics"
---

# PneumaticAtmosphere

An atmospheric (ambient-pressure) pneumatic boundary.

## Domain

A reusable **acausal pneumatic-domain** component — its compressible-gas ports carry pressure `P`, mass-flow `ṁ`, and enthalpy `h` (ISO 6358 flow). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`port`

## Usage

```
PneumaticAtmosphere inst(P, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `P` | Number | Pressure [Pa]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
port.P = P
```

## References

1. ISO 6358 — Pneumatic fluid power: flow-rate characteristics.
