---
name: dp_gravity
category: Heat Transfer
summary: dP [Pa], static head. SIDE: two-phase refrigerant in a vertical riser/downcomer. HX: evaporator/condenser vertical passes
related: []
examples: []
tags: [dp, gravity, heat, transfer]
---

# dp_gravity

dP [Pa], static head. SIDE: two-phase refrigerant in a vertical riser/downcomer. HX: evaporator/condenser vertical passes


## Syntax

```
dp_gravity(rho_l, rho_g, alpha, L, theta_deg)
```

## Description

dP [Pa], static head. SIDE: two-phase refrigerant in a vertical riser/downcomer. HX: evaporator/condenser vertical passes

## Mathematical Formulation

$$ \Delta P_{\text{grav}} = \big[\alpha\rho_g + (1-\alpha)\rho_l\big]\,g\,L\sin\theta $$

## Applicability

- **Where it applies:** Two-phase refrigerant in a vertical riser/downcomer.
- **Valid when:** Static-head term; sign follows the flow direction `θ`.
- **How it's used:** Add to the frictional and acceleration terms for the total vertical-pass `ΔP`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `rho_l` | Number | Yes | Saturated-liquid density [kg/m³]. |
| `rho_g` | Number | Yes | Saturated-vapor density [kg/m³]. |
| `alpha` | Number | Yes | Void fraction (0–1). |
| `L` | Number | Yes | Length [m]. |
| `theta_deg` | Number | Yes | Angle [deg]. |
