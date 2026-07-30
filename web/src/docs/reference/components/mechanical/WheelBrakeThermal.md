---
name: WheelBrakeThermal
category: Component (mechanical)
summary: Acausal mechanical-domain component WheelBrakeThermal with ports a, b, u.
related: []
examples: []
tags: [wheelbrakethermal, component, mechanical, acausal]
references: []
generated: true
---

# WheelBrakeThermal

Reusable acausal **mechanical-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
WheelBrakeThermal inst(Tmax, eps, C, hA, T_amb, T_fade, k_fade, eps_f, T0)
```

## Ports

`a`, `b`, `u`

## Parameters

| Parameter | Type |
| --- | --- |
| `Tmax` | Number |
| `eps` | Number |
| `C` | Number |
| `hA` | Number |
| `T_amb` | Number |
| `T_fade` | Number |
| `k_fade` | Number |
| `eps_f` | Number |
| `T0` | Number |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
fade  = 1 - k_fade * 0.5 * (1 + tanh((Tr - T_fade) / eps_f))
dw    = a.w - b.w
tau_b = fade * u.sig * Tmax * tanh(dw / eps)
a.tau = tau_b
a.tau + b.tau = 0
Pf       = tau_b * dw
der(Tr)  = (Pf - hA * (Tr - T_amb)) / C
init(Tr) = T0
```
