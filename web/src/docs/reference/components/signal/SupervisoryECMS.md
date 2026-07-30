---
name: SupervisoryECMS
category: Component (signal)
summary: Acausal signal-domain component SupervisoryECMS with ports soc, dem, eng, mot.
related: []
examples: []
tags: [supervisoryecms, component, signal, acausal]
references: []
generated: true
---

# SupervisoryECMS

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SupervisoryECMS inst(soc_ref, eps)
```

## Ports

`soc`, `dem`, `eng`, `mot`

## Parameters

| Parameter | Type |
| --- | --- |
| `soc_ref` | Number |
| `eps` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
s       = 0.5 * (1 + tanh((soc.sig - soc_ref) / eps))
mot.sig = s * dem.sig
eng.sig = (1 - s) * dem.sig
```
