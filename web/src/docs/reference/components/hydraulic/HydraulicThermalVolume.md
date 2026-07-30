---
name: HydraulicThermalVolume
category: Component (hydraulic)
summary: Acausal hydraulic-domain component HydraulicThermalVolume with ports in, out, wall.
related: []
examples: []
tags: [hydraulicthermalvolume, component, hydraulic, acausal]
references: []
generated: true
---

# HydraulicThermalVolume

Reusable acausal **hydraulic-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
HydraulicThermalVolume inst(V, rho, cp_o, beta, hA, P0, T0, Pvap, eps_c, model$, domain$)
```

## Ports

`in`, `out`, `wall`

## Parameters

| Parameter | Type |
| --- | --- |
| `V` | Number |
| `rho` | Number |
| `cp_o` | Number |
| `beta` | Number |
| `hA` | Number |
| `P0` | Number |
| `T0` | Number |
| `Pvap` | Number |
| `eps_c` | Number |
| `model$` | String |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
der(Pm)  = beta_eff / (rho * V) * (in.mdot - out.mdot)
init(Pm) = P0
T_in     = in.h / cp_o
der(Tm)  = (in.mdot * cp_o * (T_in - Tm) + hA * (wall.T - Tm)) / (rho * V * cp_o)
init(Tm) = T0
in.P     = Pm
out.P    = Pm
out.h    = cp_o * Tm
wall.Qdot = hA * (wall.T - Tm)
```

## Model Variants

Selected via the `model$` parameter; each adds its own equations (and `REQUIRE`d parameters):

### `stiff`

```
beta_eff = beta
```

### `cav` — requires `Pvap`, `eps_c`

```
beta_eff = beta * 0.5 * (1 + tanh((Pm - Pvap) / eps_c))
```
