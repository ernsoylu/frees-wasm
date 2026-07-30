---
name: EjectorMomentum
category: Component (twophase)
summary: Acausal twophase-domain component EjectorMomentum with ports mot_in, suc_in, out.
related: []
examples: []
tags: [ejectormomentum, component, twophase, acausal]
references: []
generated: true
---

# EjectorMomentum

Reusable acausal **twophase-domain** component. Instantiate it and connect its ports; instantiation expands the constitutive equations below into scalar equations solved by the standard Newton/Tarjan pipeline.

> **Auto-generated** from the component library (`backend/core/src/main/resources/components/`). The ports, parameters, and constitutive equations are taken verbatim from the component definition; a worked example and prose discussion are added as the page is curated.

## Usage

```
EjectorMomentum inst(fluid$, eta_n, eta_m, domain$)
```

## Ports

`mot_in`, `suc_in`, `out`

## Parameters

| Parameter | Type |
| --- | --- |
| `fluid$` | String |
| `eta_n` | Number |
| `eta_m` | Number |
| `domain$` | String |

## Constitutive Equations

The acausal equations this component expands into (over its port members and parameters):

```
s_m     = Entropy(fluid$, P=mot_in.P, h=mot_in.h)
h_mi    = Enthalpy(fluid$, P=suc_in.P, s=s_m)
v_m     = sqrt(2 * eta_n * (mot_in.h - h_mi))
out.mdot = mot_in.mdot + suc_in.mdot
v_mix   = eta_m * mot_in.mdot * v_m / out.mdot
out.mdot * out.h = mot_in.mdot * mot_in.h + suc_in.mdot * suc_in.h
h_mix   = out.h - v_mix^2 / 2
rho_mix = Density(fluid$, P=suc_in.P, h=h_mix)
out.P   = suc_in.P + rho_mix * v_mix^2 / 2
```
