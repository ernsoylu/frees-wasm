---
name: HydraulicTank
category: Component (hydraulic)
summary: A hydraulic reservoir at (near) atmospheric pressure.
related: []
examples: []
tags: [hydraulictank, component, hydraulic, acausal]
---

# HydraulicTank

A hydraulic reservoir at (near) atmospheric pressure.

## Domain

A reusable **acausal hydraulic-domain** component — its oil-hydraulic ports carry pressure `P`, mass-flow `ṁ`, and enthalpy `h`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`port`

## Usage

```
HydraulicTank inst(P, domain$)
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
