---
name: ProportionalReliefValve
category: Component (twophase)
summary: A pressure-relief valve whose opening rises proportionally above the set pressure.
related: []
examples: [pressure-cooker]
tags: [proportionalreliefvalve, component, twophase, acausal]
---

# ProportionalReliefValve

A pressure-relief valve whose opening rises proportionally above the set pressure.

## Domain

A reusable **acausal twophase-domain** component — its two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties). Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
ProportionalReliefValve inst(fluid$, Pcrack, grad, eps, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `fluid$` | String | Fluid name (e.g. Water, R134a, Air). |
| `Pcrack` | Number | Cracking (relief) pressure [Pa]. |
| `grad` | Number | Road grade (rise/run). |
| `eps` | Number | Effectiveness / roughness. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.h    = in.h
dpv      = in.P - Pcrack
in.mdot  = grad * 0.5 * (dpv + sqrt(dpv * dpv + eps * eps))
```

## Examples

Instantiated in the verified example below:

[Run: pressure-cooker]
