---
name: Combustor
category: Component (fluid)
summary: Acausal fluid-domain component Combustor with ports in, out.
related: []
examples: []
tags: [combustor, component, fluid, acausal]
references: []
generated: true
---

# Combustor

Reusable acausal **fluid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
Combustor inst(mdot_f, LHV, eta_b, dP)
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

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = in.mdot + mdot_f
out.P    = in.P - dP
out.mdot * out.h = in.mdot * in.h + eta_b * mdot_f * LHV
```
