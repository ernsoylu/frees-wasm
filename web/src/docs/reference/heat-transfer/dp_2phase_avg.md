---
name: dp_2phase_avg
category: Heat Transfer
summary: dP [Pa], quality-integrated (n cells). SIDE: two-phase refrigerant along an evaporator/condenser pass
related: []
examples: []
tags: [dp, 2phase, avg, heat, transfer]
---

# dp_2phase_avg

dP [Pa], quality-integrated (n cells). SIDE: two-phase refrigerant along an evaporator/condenser pass


## Syntax

```
dp_2phase_avg(fluid$, P, x_in, x_out, mdot, Dh, Aflow, L, n)
```

## Description

dP [Pa], quality-integrated (n cells). SIDE: two-phase refrigerant along an evaporator/condenser pass

## Mathematical Formulation

$$ \Delta P = \frac{1}{n}\sum_{i=1}^{n} \phi_l^2(x_i)\,\left(\frac{dP}{dz}\right)_{l,i} \Delta z \quad\text{(quality-integrated)} $$

## Applicability

- **Where it applies:** Two-phase refrigerant along an evaporator/condenser pass.
- **Valid when:** Integrates the two-phase multiplier over `n` quality cells from `x_in` to `x_out`.
- **How it's used:** A quality-averaged frictional `ΔP` for a whole pass.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `fluid$` | String | Yes | Fluid name (e.g. Water, R134a, Air). |
| `P` | Number | Yes | Pressure [Pa]. |
| `x_in` | Number | Yes | Inlet vapor quality (0–1). |
| `x_out` | Number | Yes | Outlet vapor quality (0–1). |
| `mdot` | Number | Yes | Mass flow rate [kg/s]. |
| `Dh` | Number | Yes | Hydraulic diameter [m]. |
| `Aflow` | Number | Yes | Free-flow (minimum) cross-sectional area [m²]. |
| `L` | Number | Yes | Length [m]. |
| `n` | Number | Yes | Order / number of terms. |
