---
name: HybridPowerSplit
category: Component (powertrain)
summary: Acausal powertrain-domain component HybridPowerSplit with ports eng, out, sun, p, n, u1, u2, heat.
related: []
examples: []
tags: [hybridpowersplit, component, powertrain, acausal]
references: []
generated: true
---

# HybridPowerSplit

Reusable acausal **powertrain-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
HybridPowerSplit inst(g, eff1$, eff2$, epsP)
```

## Ports

`eng`, `out`, `sun`, `p`, `n`, `u1`, `u2`, `heat`

## Parameters

| Parameter | Type |
| --- | --- |
| `g` | Number |
| `eff1$` | String |
| `eff2$` | String |
| `epsP` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
Planetary PL(g=g)
MotorMap  MG1(eff$=eff1$, epsP=epsP)
MotorMap  MG2(eff$=eff2$, epsP=epsP)
connect(eng, PL.carrier)
connect(PL.sun, MG1.shaft, sun)
connect(PL.ring, MG2.shaft, out)
connect(MG1.p, MG2.p, p)
connect(MG1.n, MG2.n, n)
connect(MG1.u, u1)
connect(MG2.u, u2)
connect(MG1.heat, MG2.heat, heat)
```
