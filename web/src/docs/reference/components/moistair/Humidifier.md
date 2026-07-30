---
name: Humidifier
category: Component (moistair)
summary: Adds moisture to a humid-air stream, raising its humidity ratio.
related: []
examples: []
tags: [humidifier, component, moistair, acausal]
---

# Humidifier

Adds moisture to a humid-air stream, raising its humidity ratio.

## Domain

A reusable **acausal moistair-domain** component — its humid-air ports carry pressure `P`, dry-air mass-flow `ṁ_da`, enthalpy `h`, and humidity ratio `W`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`, `out`

## Usage

```
Humidifier inst(mdot_w, h_w, domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `mdot_w` | Number | Water/coolant mass flow [kg/s]. |
| `h_w` | Number | Wall heat-transfer coefficient [W/m²·K]. |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.mdot = in.mdot
out.P    = in.P
out.W    = in.W + mdot_w / in.mdot
out.h    = in.h + mdot_w * h_w / in.mdot
```
