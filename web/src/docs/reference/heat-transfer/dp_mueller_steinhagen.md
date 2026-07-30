---
name: dp_mueller_steinhagen
category: Heat Transfer
summary: dP [Pa], Mueller-Steinhagen-Heck. SIDE: two-phase refrigerant (alt to dp_2phase). HX: evaporator/condenser refrigerant line
related: []
examples: []
tags: [dp, mueller, steinhagen, heat, transfer]
---

# dp_mueller_steinhagen

dP [Pa], Mueller-Steinhagen-Heck. SIDE: two-phase refrigerant (alt to dp_2phase). HX: evaporator/condenser refrigerant line


## Syntax

```
dp_mueller_steinhagen(fluid$, P, x, mdot, Dh, Aflow, L)
```

## Description

dP [Pa], Mueller-Steinhagen-Heck. SIDE: two-phase refrigerant (alt to dp_2phase). HX: evaporator/condenser refrigerant line

## Mathematical Formulation

$$ \frac{dP}{dz} = G_{ms}(1-x)^{1/3} + B\,x^3, \quad G_{ms} = A + 2(B-A)x \quad\text{(Müller-Steinhagen–Heck)} $$

## Applicability

- **Where it applies:** Two-phase refrigerant in an evaporator/condenser line.
- **Valid when:** Two-phase frictional drop; an alternative to the Chisholm/Friedel route (`dp_2phase`).
- **How it's used:** Interpolates between the all-liquid and all-vapor drops over the quality range.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `fluid$` | String | Yes | Fluid name (e.g. Water, R134a, Air). |
| `P` | Number | Yes | Pressure [Pa]. |
| `x` | Number | Yes | Vapor quality (0–1). |
| `mdot` | Number | Yes | Mass flow rate [kg/s]. |
| `Dh` | Number | Yes | Hydraulic diameter [m]. |
| `Aflow` | Number | Yes | Free-flow (minimum) cross-sectional area [m²]. |
| `L` | Number | Yes | Length [m]. |
