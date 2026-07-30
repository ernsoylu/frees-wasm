---
name: lm_martinelli_tt
category: Two-Phase Flow
summary: Turbulent-turbulent Martinelli parameter X_tt
related: []
examples: []
tags: [lm, martinelli, tt, two, phase, flow]
---

# lm_martinelli_tt

Turbulent-turbulent Martinelli parameter X_tt


## Syntax

```
lm_martinelli_tt(x, rho_l, rho_g, mu_l, mu_g)
```

## Description

Returns the **turbulent–turbulent Lockhart–Martinelli parameter `X_tt`** — the ratio of the liquid-alone to vapor-alone pressure gradients that two-phase correlations key on.

## Mathematical Formulation

$$ X_{tt} = \left(\frac{1-x}{x}\right)^{0.9}\left(\frac{\rho_g}{\rho_l}\right)^{0.5}\left(\frac{\mu_l}{\mu_g}\right)^{0.1} $$

## Applicability

- **Where it applies:** The independent variable for two-phase heat-transfer and pressure-drop correlations.
- **Valid when:** Both phases turbulent (the usual refrigerant case).
- **How it's used:** Feeds `lm_phi2`, the Chen factors, and many two-phase Nusselt correlations.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Vapor quality (0–1). |
| `rho_l` | Number | Yes | Saturated-liquid density [kg/m³]. |
| `rho_g` | Number | Yes | Saturated-vapor density [kg/m³]. |
| `mu_l` | Number | Yes | Liquid dynamic viscosity [Pa·s]. |
| `mu_g` | Number | Yes | Vapor dynamic viscosity [Pa·s]. |
