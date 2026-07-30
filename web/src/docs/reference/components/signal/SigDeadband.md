---
name: SigDeadband
category: Component (signal)
summary: Acausal signal-domain component SigDeadband with ports in, out.
related: []
examples: []
tags: [sigdeadband, component, signal, acausal]
references: []
generated: true
---

# SigDeadband

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigDeadband inst(w, eps)
```

## Ports

`in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `w` | Number |
| `eps` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
up      = in.sig - w
dn      = in.sig + w
out.sig = 0.5 * (up + sqrt(up^2 + eps^2)) + 0.5 * (dn - sqrt(dn^2 + eps^2))
```
