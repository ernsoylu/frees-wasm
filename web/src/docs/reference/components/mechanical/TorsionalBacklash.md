---
name: TorsionalBacklash
category: Component (mechanical)
summary: Acausal mechanical-domain component TorsionalBacklash with ports a, b.
related: []
examples: []
tags: [torsionalbacklash, component, mechanical, acausal]
references: []
generated: true
---

# TorsionalBacklash

Reusable acausal **mechanical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
TorsionalBacklash inst(k, half, eps, theta0)
```

## Ports

`a`, `b`

## Parameters

| Parameter | Type |
| --- | --- |
| `k` | Number |
| `half` | Number |
| `eps` | Number |
| `theta0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(th)  = a.w - b.w
init(th) = theta0
up       = th - half
dn       = th + half
a.tau    = k * (0.5 * (up + sqrt(up^2 + eps^2)) + 0.5 * (dn - sqrt(dn^2 + eps^2)))
a.tau + b.tau = 0
```
