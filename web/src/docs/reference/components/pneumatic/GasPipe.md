---
name: GasPipe
category: Component (pneumatic)
summary: A pneumatic pipe with compressible-flow pressure drop.
related: []
examples: []
tags: [gaspipe, component, pneumatic, acausal]
references:
  - "ISO 6358 — Pneumatic fluid power: flow-rate characteristics"
---

# GasPipe

A pneumatic pipe with compressible-flow pressure drop.

## Domain

A reusable **acausal pneumatic-domain** component — its compressible-gas ports carry pressure `P`, mass-flow `ṁ`, and enthalpy `h` (ISO 6358 flow). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
GasPipe inst(...)
```

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.P    = in.P
out.h    = in.h
out.y    = in.y
```

## References

1. ISO 6358 — Pneumatic fluid power: flow-rate characteristics.
