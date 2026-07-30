---
name: GasMixer
category: Component (pneumatic)
summary: Mixes pneumatic gas streams, carrying the species composition rider.
related: []
examples: []
tags: [gasmixer, component, pneumatic, acausal]
references:
  - "ISO 6358 — Pneumatic fluid power: flow-rate characteristics"
---

# GasMixer

Mixes pneumatic gas streams, carrying the species composition rider.

## Domain

A reusable **acausal pneumatic-domain** component — its compressible-gas ports carry pressure `P`, mass-flow `ṁ`, and enthalpy `h` (ISO 6358 flow). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in1`, `in2`, `out`

## Usage

```
GasMixer inst(...)
```

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.P    = in1.P
out.mdot = in1.mdot + in2.mdot
out.mdot * out.h = in1.mdot * in1.h + in2.mdot * in2.h
out.mdot * out.y = in1.mdot * in1.y + in2.mdot * in2.y
```

## References

1. ISO 6358 — Pneumatic fluid power: flow-rate characteristics.
