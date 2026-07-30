---
name: dp_compact_core
category: Heat Transfer
summary: dP [Pa], compact-core (entrance/accel/core-friction/exit). SIDE: air/gas through a compact finned core. HX: fin-and-tube/plate-fin radiator, condenser, CAC air side
related: []
examples: []
tags: [dp, compact, core, heat, transfer]
---

# dp_compact_core

dP [Pa], compact-core (entrance/accel/core-friction/exit). SIDE: air/gas through a compact finned core. HX: fin-and-tube/plate-fin radiator, condenser, CAC air side


## Syntax

```
dp_compact_core(G, rho_in, rho_out, rho_mean, sigma, f, AoverAc, Kc, Ke)
```

## Description

dP [Pa], compact-core (entrance/accel/core-friction/exit). SIDE: air/gas through a compact finned core. HX: fin-and-tube/plate-fin radiator, condenser, CAC air side

## Mathematical Formulation

$$ \frac{\Delta P}{P_1} = \frac{G^2}{2\rho_1 P_1}\left[(1+\sigma^2)\!\left(\tfrac{\rho_1}{\rho_2}-1\right) + f\tfrac{A}{A_c}\tfrac{\rho_1}{\rho_m}\right] \quad\text{(compact core)} $$

## Applicability

- **Where it applies:** Air/gas through a compact finned core.
- **Valid when:** Includes the entrance, acceleration, core-friction and exit terms.
- **How it's used:** Air-side `ΔP` for a fin-and-tube / plate-fin radiator, condenser, or CAC.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `G` | Number | Yes | Mass flux G = ṁ/Aflow [kg/m²·s]. |
| `rho_in` | Number | Yes | Inlet density [kg/m³]. |
| `rho_out` | Number | Yes | Outlet density [kg/m³]. |
| `rho_mean` | Number | Yes | Mean density [kg/m³]. |
| `sigma` | Number | Yes | Surface tension [N/m]. |
| `f` | Number | Yes | Fanning/Darcy friction factor. |
| `AoverAc` | Number | Yes | Area ratio A/Ac. |
| `Kc` | Number | Yes | Contraction (entrance) loss coefficient. |
| `Ke` | Number | Yes | Exit (expansion) loss coefficient. |
