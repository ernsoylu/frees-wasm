---
name: MotorMap
category: Component (electrical)
summary: Acausal electrical-domain component MotorMap with ports p, n, shaft, heat, u.
related: []
examples: []
tags: [motormap, component, electrical, acausal]
references: []
generated: true
---

# MotorMap

Reusable acausal **electrical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
MotorMap inst(eff$, epsP)
```

## Ports

`p`, `n`, `shaft`, `heat`, `u`

## Parameters

| Parameter | Type |
| --- | --- |
| `eff$` | String |
| `epsP` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
tau_e     = u.sig
shaft.tau = -tau_e
Pm        = tau_e * shaft.w
eff       = eff$(shaft.w, tau_e)
s         = 0.5 * (1 + tanh(Pm / epsP))
Pe        = s * Pm / eff + (1 - s) * Pm * eff
(p.V - n.V) * p.I = Pe
p.I + n.I = 0
Q         = Pe - Pm
heat.Qdot = -Q
```
