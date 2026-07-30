---
name: LoadSensingPump
category: Component (hydraulic)
summary: Acausal hydraulic-domain component LoadSensingPump with ports in, out, ls.
related: []
examples: []
tags: [loadsensingpump, component, hydraulic, acausal]
references: []
generated: true
---

# LoadSensingPump

Reusable acausal **hydraulic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
LoadSensingPump inst(rho, Dv, w_p, dP_margin, tau, d0, domain$)
```

## Ports

`in`, `out`, `ls`

## Parameters

| Parameter | Type |
| --- | --- |
| `rho` | Number |
| `Dv` | Number |
| `w_p` | Number |
| `dP_margin` | Number |
| `tau` | Number |
| `d0` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(dfrac)  = ((ls.P + dP_margin) - out.P) / (tau * dP_margin)
init(dfrac) = d0
deff     = 1 / (1 + exp(-8 * (dfrac - 0.5)))
out.mdot = deff * rho * Dv * w_p / (2 * pi#)
in.mdot  = out.mdot
out.h    = in.h
ls.mdot  = 0
```
