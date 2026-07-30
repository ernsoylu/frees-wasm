---
name: GasMixerN
category: Component (pneumatic)
summary: Acausal pneumatic-domain component GasMixerN with ports in1, in2, out.
related: []
examples: []
tags: [gasmixern, component, pneumatic, acausal]
references: []
generated: true
---

# GasMixerN

Reusable acausal **pneumatic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
GasMixerN inst(domain$)
```

## Ports

`in1`, `in2`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.P    = in1.P
out.mdot = in1.mdot + in2.mdot
out.mdot * out.h    = in1.mdot * in1.h    + in2.mdot * in2.h
out.mdot * out.yo2  = in1.mdot * in1.yo2  + in2.mdot * in2.yo2
out.mdot * out.yco2 = in1.mdot * in1.yco2 + in2.mdot * in2.yco2
out.mdot * out.yh2o = in1.mdot * in1.yh2o + in2.mdot * in2.yh2o
out.mdot * out.yn2  = in1.mdot * in1.yn2  + in2.mdot * in2.yn2
```
