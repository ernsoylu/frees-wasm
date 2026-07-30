---
name: OpenTank
category: Component (liquid)
summary: Acausal liquid-domain component OpenTank with ports in, out.
related: []
examples: []
tags: [opentank, component, liquid, acausal]
references: []
generated: true
---

# OpenTank

Reusable acausal **liquid-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
OpenTank inst(A_t, P0, rho, L0, domain$)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `A_t` | Number |
| `P0` | Number |
| `rho` | Number |
| `L0` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(lvl)  = (in.mdot - out.mdot) / (rho * A_t)
init(lvl) = L0
in.P  = P0
out.P = P0 + rho * 9.80665 * lvl
out.h = in.h
```
