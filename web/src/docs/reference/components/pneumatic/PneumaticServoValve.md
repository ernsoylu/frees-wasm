---
name: PneumaticServoValve
category: Component (pneumatic)
summary: A pneumatic servo valve with a commanded spool position.
related: []
examples: []
tags: [pneumaticservovalve, component, pneumatic, acausal]
references:
  - "ISO 6358 — Pneumatic fluid power: flow-rate characteristics"
---

# PneumaticServoValve

A pneumatic servo valve with a commanded spool position.

## Domain

A reusable **acausal pneumatic-domain** component — its compressible-gas ports carry pressure `P`, mass-flow `ṁ`, and enthalpy `h` (ISO 6358 flow). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
PneumaticServoValve inst(fluid$, Cmax, b, u, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `Cmax` | Number | Maximum capacity rate [W/K]. |
| `b` | Number | Critical pressure ratio / coefficient. |
| `u` | Number | Specific internal energy [J/kg]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.h    = in.h
T_in     = Temperature(fluid$, P=in.P, h=in.h)
in.mdot  = iso6358(u * Cmax, b, in.P, T_in, out.P)
out.mdot = in.mdot
```

## References

1. ISO 6358 — Pneumatic fluid power: flow-rate characteristics.
