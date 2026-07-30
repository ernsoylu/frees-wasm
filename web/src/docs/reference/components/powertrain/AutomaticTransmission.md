---
name: AutomaticTransmission
category: Component (powertrain)
summary: Acausal powertrain-domain component AutomaticTransmission with ports in, out, gear, lock.
related: []
examples: []
tags: [automatictransmission, component, powertrain, acausal]
references: []
generated: true
---

# AutomaticTransmission

Reusable acausal **powertrain-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
AutomaticTransmission inst(Kmap$, TRmap$, eta, Tlock, eps)
```

## Ports

`in`, `out`, `gear`, `lock`

## Parameters

| Parameter | Type |
| --- | --- |
| `Kmap$` | String |
| `TRmap$` | String |
| `eta` | Number |
| `Tlock` | Number |
| `eps` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
TorqueConverter  TC(Kmap$=Kmap$, TRmap$=TRmap$)
GearboxScheduled GB(eta=eta)
ClutchCmd        LU(Tmax=Tlock, eps=eps)
connect(in, TC.pump, LU.a)
connect(TC.turb, LU.b, GB.in)
connect(GB.out, out)
connect(gear, GB.u)
connect(lock, LU.u)
```
