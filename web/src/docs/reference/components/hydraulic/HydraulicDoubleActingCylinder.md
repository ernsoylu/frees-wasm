---
name: HydraulicDoubleActingCylinder
category: Component (hydraulic)
summary: Acausal hydraulic-domain component HydraulicDoubleActingCylinder with ports a, b, rod.
related: []
examples: []
tags: [hydraulicdoubleactingcylinder, component, hydraulic, acausal]
references: []
generated: true
---

# HydraulicDoubleActingCylinder

Reusable acausal **hydraulic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
HydraulicDoubleActingCylinder inst(Aa, Ab, rho, beta, Va0, Vb0, Pa0, Pb0, domain$)
```

## Ports

`a`, `b`, `rod`

## Parameters

| Parameter | Type |
| --- | --- |
| `Aa` | Number |
| `Ab` | Number |
| `rho` | Number |
| `beta` | Number |
| `Va0` | Number |
| `Vb0` | Number |
| `Pa0` | Number |
| `Pb0` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
rod.f     = -(a.P * Aa - b.P * Ab)
der(a.P)  = (beta / Va0) * (a.mdot / rho - Aa * rod.vel)
init(a.P) = Pa0
der(b.P)  = (beta / Vb0) * (b.mdot / rho + Ab * rod.vel)
init(b.P) = Pb0
```
