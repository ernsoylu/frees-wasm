---
name: GradeProfile
category: Component (powertrain)
summary: Acausal powertrain-domain component GradeProfile with ports port.
related: []
examples: []
tags: [gradeprofile, component, powertrain, acausal]
references: []
generated: true
---

# GradeProfile

Reusable acausal **powertrain-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
GradeProfile inst(m, g, map$, s0)
```

## Ports

`port`

## Parameters

| Parameter | Type |
| --- | --- |
| `m` | Number |
| `g` | Number |
| `map$` | String |
| `s0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(s)  = port.vel
init(s) = s0
port.f  = m * g * sin(map$(s))
```
