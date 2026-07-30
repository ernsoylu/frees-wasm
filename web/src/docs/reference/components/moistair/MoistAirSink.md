---
name: MoistAirSink
category: Component (moistair)
summary: A humid-air boundary absorbing a stream.
related: []
examples: []
tags: [moistairsink, component, moistair, acausal]
---

# MoistAirSink

A humid-air boundary absorbing a stream.

## Domain

A reusable **acausal moistair-domain** component — its humid-air ports carry pressure `P`, dry-air mass-flow `ṁ_da`, enthalpy `h`, and humidity ratio `W`. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.

## Ports

`in`

## Usage

```
MoistAirSink inst(domain$)
```

## Parameters

| Parameter | Type | Description |
| --- | --- | --- |
| `domain$` | String | Connector fluid family — one of `fluid`, `gas`, `oil`, `moistair`, `liquid`, `twophase`. |

## Constitutive Equations

Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:

```
mdot = in.mdot
P    = in.P
h    = in.h
W    = in.W
```
