---
name: TorqueConverter
category: Component (powertrain)
summary: Acausal powertrain-domain component TorqueConverter with ports pump, turb.
related: []
examples: []
tags: [torqueconverter, component, powertrain, acausal]
references: []
generated: true
---

# TorqueConverter

Reusable acausal **powertrain-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
TorqueConverter inst(Kmap$, TRmap$)
```

## Ports

`pump`, `turb`

## Parameters

| Parameter | Type |
| --- | --- |
| `Kmap$` | String |
| `TRmap$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
SR       = turb.w / pump.w
tau_p    = (pump.w / Kmap$(SR))^2
pump.tau = tau_p
turb.tau = -TRmap$(SR) * tau_p
```
