---
name: dp_1phase
category: Heat Transfer
summary: dP [Pa], Darcy. SIDE: single-phase liquid/gas line (coolant, water, air channel, pipe). HX: radiator/CAC fluid channels
related: []
examples: []
tags: [dp, 1phase, heat, transfer]
---

# dp_1phase

dP [Pa], Darcy. SIDE: single-phase liquid/gas line (coolant, water, air channel, pipe). HX: radiator/CAC fluid channels


## Syntax

```
dp_1phase(fluid$, P, T, mdot, Dh, Aflow, L)
```

## Description

dP [Pa], Darcy. SIDE: single-phase liquid/gas line (coolant, water, air channel, pipe). HX: radiator/CAC fluid channels

## Mathematical Formulation

$$ \Delta P = f\,\frac{L}{D_h}\,\frac{G^2}{2\rho}, \qquad G = \dot m / A_{\text{flow}} \quad\text{(Darcy)} $$

## Applicability

- **Where it applies:** A single-phase liquid/gas line (coolant, water, air channel, pipe).
- **Valid when:** Single-phase Darcy flow; turbulent or laminar.
- **How it's used:** Friction `ΔP` for radiator/CAC fluid channels and connecting lines.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `fluid$` | String | Yes | Fluid name (e.g. Water, R134a, Air). |
| `P` | Number | Yes | Pressure [Pa]. |
| `T` | Number | Yes | Temperature [K]. |
| `mdot` | Number | Yes | Mass flow rate [kg/s]. |
| `Dh` | Number | Yes | Hydraulic diameter [m]. |
| `Aflow` | Number | Yes | Free-flow (minimum) cross-sectional area [m²]. |
| `L` | Number | Yes | Length [m]. |
