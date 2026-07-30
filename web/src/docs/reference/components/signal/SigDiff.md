---
name: SigDiff
category: Component (signal)
summary: Acausal signal-domain component SigDiff with ports in1, in2, out.
related: []
examples: []
tags: [sigdiff, component, signal, acausal]
references: []
generated: true
---

# SigDiff

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigDiff inst(param = value, ...)
```

## Ports

`in1`, `in2`, `out`

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.sig = in1.sig - in2.sig
```
