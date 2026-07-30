---
name: SigMax
category: Component (signal)
summary: Acausal signal-domain component SigMax with ports in1, in2, out.
related: []
examples: []
tags: [sigmax, component, signal, acausal]
references: []
generated: true
---

# SigMax

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigMax inst(param = value, ...)
```

## Ports

`in1`, `in2`, `out`

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.sig = max(in1.sig, in2.sig)
```
