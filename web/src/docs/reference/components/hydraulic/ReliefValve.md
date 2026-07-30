---
name: ReliefValve
category: Component (hydraulic)
summary: A pressure-relief valve that opens above its set pressure.
related: []
examples: []
tags: [reliefvalve, component, hydraulic, acausal]
---

# ReliefValve

A pressure-relief valve that opens above its set pressure.

## Domain

A reusable **acausal hydraulic-domain** component — its oil-hydraulic ports carry pressure `P`, mass-flow `ṁ`, and enthalpy `h`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
ReliefValve inst(Pcrack, K, eps, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `Pcrack` | Number | Cracking (relief) pressure [Pa]. |
| `K` | Number | Gain / coefficient. |
| `eps` | Number | Effectiveness / roughness. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.h    = in.h
open     = 0.5 * (1 + tanh((in.P - Pcrack) / eps))
in.mdot  = K * open * (in.P - out.P)
```
