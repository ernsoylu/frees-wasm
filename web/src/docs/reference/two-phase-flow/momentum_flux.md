---
name: momentum_flux
category: Two-Phase Flow
summary: Separated-flow momentum flux [Pa] (accel. dP = out-in)
related: []
examples: []
tags: [momentum, flux, two, phase, flow]
---

# momentum_flux

Separated-flow momentum flux [Pa] (accel. dP = out-in)


## Syntax

```
momentum_flux(x, rho_l, rho_g, alpha, G)
```

## Description

Returns the **separated-flow momentum flux** — the acceleration pressure change (outlet − inlet) caused by the change in vapor quality, not by friction.

## Mathematical Formulation

$$ \left(\frac{d P}{d z}\right)_{\text{acc}} = G^2\frac{d}{dz}\left[\frac{x^2}{\rho_g\alpha} + \frac{(1-x)^2}{\rho_l(1-\alpha)}\right] $$

## Applicability

- **Where it applies:** The acceleration `ΔP` term along an evaporator/condenser pass.
- **Valid when:** Wherever quality changes appreciably (vapor generation in an evaporator accelerates the flow).
- **How it's used:** Add it to the frictional (`lm_phi2`) and gravitational (`dp_gravity`) terms for the total pass `ΔP`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Vapor quality (0–1). |
| `rho_l` | Number | Yes | Saturated-liquid density [kg/m³]. |
| `rho_g` | Number | Yes | Saturated-vapor density [kg/m³]. |
| `alpha` | Number | Yes | Void fraction (0–1). |
| `G` | Number | Yes | Mass flux G = ṁ/Aflow [kg/m²·s]. |
