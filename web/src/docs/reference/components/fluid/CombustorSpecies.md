---
name: CombustorSpecies
category: Component (fluid)
summary: Acausal fluid-domain component CombustorSpecies with ports in, out.
related: []
examples: []
tags: [combustorspecies, component, fluid, acausal]
references: []
generated: true
---

# CombustorSpecies

Reusable acausal **fluid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
CombustorSpecies inst(mdot_f, LHV, eta_b, dP, xC, yH, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `mdot_f` | Number |
| `LHV` | Number |
| `eta_b` | Number |
| `dP` | Number |
| `xC` | Number |
| `yH` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
mfuel    = 12 * xC + yH
mCO2     = mdot_f * 44 * xC / mfuel
mH2O     = mdot_f * 9 * yH / mfuel
mO2      = mdot_f * 32 * (xC + yH / 4) / mfuel
out.mdot = in.mdot + mdot_f
out.P    = in.P - dP
out.mdot * out.h    = in.mdot * in.h + eta_b * mdot_f * LHV
out.mdot * out.yco2 = in.mdot * in.yco2 + mCO2
out.mdot * out.yh2o = in.mdot * in.yh2o + mH2O
out.mdot * out.yo2  = in.mdot * in.yo2  - mO2
out.mdot * out.yn2  = in.mdot * in.yn2
```
