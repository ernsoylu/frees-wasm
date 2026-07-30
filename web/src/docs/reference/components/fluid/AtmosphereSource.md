---
name: AtmosphereSource
category: Component (fluid)
summary: Acausal fluid-domain component AtmosphereSource with ports out.
related: []
examples: []
tags: [atmospheresource, component, fluid, acausal]
references: []
generated: true
---

# AtmosphereSource

Reusable acausal **fluid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
AtmosphereSource inst(alt, mdot)
```

## Ports

`out`

## Parameters

| Parameter | Type |
| --- | --- |
| `alt` | Number |
| `mdot` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.mdot = mdot
out.P    = isa_P(alt)
out.h    = Enthalpy(Air, P=isa_P(alt), T=isa_T(alt))
```
