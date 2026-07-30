---
name: SigThermalProbe
category: Component (signal)
summary: Acausal signal-domain component SigThermalProbe with ports port, out.
related: []
examples: []
tags: [sigthermalprobe, component, signal, acausal]
references: []
generated: true
---

# SigThermalProbe

Reusable acausal **signal-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
SigThermalProbe inst(param = value, ...)
```

## Ports

`port`, `out`

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
port.Qdot = 0
out.sig   = port.T
```
