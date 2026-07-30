---
name: TranscriticalBackPressureValve
category: Component (twophase)
summary: Acausal twophase-domain component TranscriticalBackPressureValve with ports in, out, u.
related: []
examples: []
tags: [transcriticalbackpressurevalve, component, twophase, acausal]
references: []
generated: true
---

# TranscriticalBackPressureValve

Reusable acausal **twophase-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
TranscriticalBackPressureValve inst(fluid$, CdA_max, domain$)
```

## Ports

`in`, `out`, `u`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `CdA_max` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
rho      = Density(fluid$, P=in.P, h=in.h)
out.mdot = in.mdot
out.h    = in.h
in.mdot * abs(in.mdot) = (u.sig * CdA_max)^2 * 2 * rho * (in.P - out.P)
```
