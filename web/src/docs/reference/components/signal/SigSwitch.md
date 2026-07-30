---
name: SigSwitch
category: Component (signal)
summary: Acausal signal-domain component SigSwitch with ports in1, in2, ctrl, out.
related: []
examples: []
tags: [sigswitch, component, signal, acausal]
references: []
generated: true
---

# SigSwitch

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigSwitch inst(thresh, eps)
```

## Ports

`in1`, `in2`, `ctrl`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `thresh` | Number |
| `eps` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
wgt     = 0.5 * (1 + tanh((ctrl.sig - thresh) / eps))
out.sig = wgt * in1.sig + (1 - wgt) * in2.sig
```
