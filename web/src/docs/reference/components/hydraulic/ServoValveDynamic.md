---
name: ServoValveDynamic
category: Component (hydraulic)
summary: Acausal hydraulic-domain component ServoValveDynamic with ports in, out, u.
related: []
examples: []
tags: [servovalvedynamic, component, hydraulic, acausal]
references: []
generated: true
---

# ServoValveDynamic

Reusable acausal **hydraulic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
ServoValveDynamic inst(CdA_max, rho, wn, zeta, xs0, domain$)
```

## Ports

`in`, `out`, `u`

## Parameters

| Parameter | Type |
| --- | --- |
| `CdA_max` | Number |
| `rho` | Number |
| `wn` | Number |
| `zeta` | Number |
| `xs0` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(xs)  = vs
init(xs) = xs0
der(vs)  = wn^2 * (u.sig - xs) - 2 * zeta * wn * vs
init(vs) = 0
out.mdot = in.mdot
out.h    = in.h
in.mdot * abs(in.mdot) = (xs * CdA_max)^2 * 2 * rho * (in.P - out.P)
```
