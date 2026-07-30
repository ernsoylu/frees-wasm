---
name: PneumaticCheckValve
category: Component (pneumatic)
summary: Acausal pneumatic-domain component PneumaticCheckValve with ports in, out.
related: []
examples: []
tags: [pneumaticcheckvalve, component, pneumatic, acausal]
references: []
generated: true
---

# PneumaticCheckValve

Reusable acausal **pneumatic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
PneumaticCheckValve inst(fluid$, C, b, eps, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `C` | Number |
| `b` | Number |
| `eps` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot
out.h    = in.h
g        = 0.5 * (1 + tanh((in.P - out.P) / eps))
T_in     = Temperature(fluid$, P=in.P, h=in.h)
in.mdot  = g * iso6358(C, b, in.P, T_in, out.P)
```
