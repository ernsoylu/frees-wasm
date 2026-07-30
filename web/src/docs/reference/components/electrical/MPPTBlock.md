---
name: MPPTBlock
category: Component (electrical)
summary: Acausal electrical-domain component MPPTBlock with ports G, out.
related: []
examples: []
tags: [mpptblock, component, electrical, acausal]
references: []
generated: true
---

# MPPTBlock

Reusable acausal **electrical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
MPPTBlock inst(vmp$)
```

## Ports

`G`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `vmp$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
out.sig = vmp$(G.sig)
```
