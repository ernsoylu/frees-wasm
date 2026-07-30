---
name: PneumaticDoubleActingCylinder
category: Component (pneumatic)
summary: Acausal pneumatic-domain component PneumaticDoubleActingCylinder with ports a, b, rod.
related: []
examples: []
tags: [pneumaticdoubleactingcylinder, component, pneumatic, acausal]
references: []
generated: true
---

# PneumaticDoubleActingCylinder

Reusable acausal **pneumatic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
PneumaticDoubleActingCylinder inst(Aa, Ab, R, T, Va0, Vb0, Pa0, Pb0, domain$)
```

## Ports

`a`, `b`, `rod`

## Parameters

| Parameter | Type |
| --- | --- |
| `Aa` | Number |
| `Ab` | Number |
| `R` | Number |
| `T` | Number |
| `Va0` | Number |
| `Vb0` | Number |
| `Pa0` | Number |
| `Pb0` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
rod.f     = -(a.P * Aa - b.P * Ab)
der(a.P)  = (R * T * a.mdot - a.P * Aa * rod.vel) / Va0
init(a.P) = Pa0
der(b.P)  = (R * T * b.mdot + b.P * Ab * rod.vel) / Vb0
init(b.P) = Pb0
```
