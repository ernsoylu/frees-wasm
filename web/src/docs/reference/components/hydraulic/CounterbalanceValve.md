---
name: CounterbalanceValve
category: Component (hydraulic)
summary: Acausal hydraulic-domain component CounterbalanceValve with ports in, out, pilot.
related: []
examples: []
tags: [counterbalancevalve, component, hydraulic, acausal]
references: []
generated: true
---

# CounterbalanceValve

Reusable acausal **hydraulic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
CounterbalanceValve inst(CdA_max, rho, P_set, R_p, eps_o, domain$)
```

## Ports

`in`, `out`, `pilot`

## Parameters

| Parameter | Type |
| --- | --- |
| `CdA_max` | Number |
| `rho` | Number |
| `P_set` | Number |
| `R_p` | Number |
| `eps_o` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
x_o      = 0.5 * (1 + tanh((in.P + R_p * pilot.P - P_set) / eps_o))
out.mdot = in.mdot
out.h    = in.h
pilot.mdot = 0
in.mdot * abs(in.mdot) = (x_o * CdA_max)^2 * 2 * rho * (in.P - out.P)
```
