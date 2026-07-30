---
name: SigVelProbe
category: Component (signal)
summary: Acausal signal-domain component SigVelProbe with ports port, out.
related: []
examples: []
tags: [sigvelprobe, component, signal, acausal]
references: []
generated: true
---

# SigVelProbe

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigVelProbe inst(param = value, ...)
```

## Ports

`port`, `out`

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
port.f  = 0
out.sig = port.vel
```
