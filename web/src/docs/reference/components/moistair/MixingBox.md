---
name: MixingBox
category: Component (moistair)
summary: Mixes two humid-air streams with flow-weighted enthalpy and humidity ratio.
related: []
examples: []
tags: [mixingbox, component, moistair, acausal]
---

# MixingBox

Mixes two humid-air streams with flow-weighted enthalpy and humidity ratio.

## Domain

A reusable **acausal moistair-domain** component — its humid-air ports carry pressure `P`, dry-air mass-flow `ṁ_da`, enthalpy `h`, and humidity ratio `W`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in1`, `in2`, `out`

## Usage

```
MixingBox inst(domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
out.P    = in1.P
out.mdot = in1.mdot + in2.mdot
out.mdot * out.W = in1.mdot * in1.W + in2.mdot * in2.W
out.mdot * out.h = in1.mdot * in1.h + in2.mdot * in2.h
```
