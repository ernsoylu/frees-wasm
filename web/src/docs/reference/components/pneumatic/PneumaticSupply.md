---
name: PneumaticSupply
category: Component (pneumatic)
summary: A pneumatic pressure supply.
related: []
examples: []
tags: [pneumaticsupply, component, pneumatic, acausal]
references:
  - "ISO 6358 — Pneumatic fluid power: flow-rate characteristics"
---

# PneumaticSupply

A pneumatic pressure supply.

## Domain

A reusable **acausal pneumatic-domain** component — its compressible-gas ports carry pressure `P`, mass-flow `ṁ`, and enthalpy `h` (ISO 6358 flow). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`out`

## Usage

```
PneumaticSupply inst(fluid$, P, T, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `P` | Number | Pressure [Pa]. |
| `T` | Number | Temperature [K]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.P = P
out.h = Enthalpy(fluid$, P=P, T=T)
```

## References

1. ISO 6358 — Pneumatic fluid power: flow-rate characteristics.
