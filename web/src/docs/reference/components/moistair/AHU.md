---
name: AHU
category: Component (moistair)
summary: Acausal moistair-domain component AHU with ports ret_in, oa_in, sup_out.
related: []
examples: []
tags: [ahu, component, moistair, acausal]
references: []
generated: true
---

# AHU

Reusable acausal **moistair-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
AHU inst(Kf, foul, Tc, Qh, dPfan, eta_fan)
```

## Ports

`ret_in`, `oa_in`, `sup_out`

## Parameters

| Parameter | Type |
| --- | --- |
| `Kf` | Number |
| `foul` | Number |
| `Tc` | Number |
| `Qh` | Number |
| `dPfan` | Number |
| `eta_fan` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
MixingBox   MB()
AirFilter   FL(K=Kf, foul=foul)
CoolingCoil CC(Tout=Tc)
HeatingCoil HC(Q=Qh)
MoistAirFan FN(dP=dPfan, eta=eta_fan)
connect(ret_in, MB.in1)
connect(oa_in, MB.in2)
connect(MB.out, FL.in)
connect(FL.out, CC.in)
connect(CC.out, HC.in)
connect(HC.out, FN.in)
connect(FN.out, sup_out)
```
