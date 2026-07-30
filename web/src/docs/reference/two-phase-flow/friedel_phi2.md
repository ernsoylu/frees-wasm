---
name: friedel_phi2
category: Two-Phase Flow
summary: Friedel two-phase frictional multiplier on the liquid-only drop
related: []
examples: []
tags: [friedel, phi2, two, phase, flow]
---

# friedel_phi2

Friedel two-phase frictional multiplier on the liquid-only drop


## Syntax

```
friedel_phi2(x, rho_l, rho_g, mu_l, mu_g, G, D, sigma)
```

## Description

Returns the **Friedel two-phase frictional multiplier** on the liquid-only pressure drop — an alternative to Chisholm that uses the Froude and Weber numbers for broader validity.

## Mathematical Formulation

$$ \phi_{lo}^2 = E + \frac{3.24\,F H}{Fr^{0.045}We^{0.035}} \quad\text{(Friedel)} $$

## Applicability

- **Where it applies:** Two-phase frictional pressure drop in refrigerant passages.
- **Valid when:** Recommended for `μ_l/μ_g < 1000`; covers a wider mass-flux range than the simple Chisholm form.
- **How it's used:** Multiply the liquid-only gradient by the multiplier to get the two-phase `ΔP`; an alternative to `lm_phi2`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Vapor quality (0–1). |
| `rho_l` | Number | Yes | Saturated-liquid density [kg/m³]. |
| `rho_g` | Number | Yes | Saturated-vapor density [kg/m³]. |
| `mu_l` | Number | Yes | Liquid dynamic viscosity [Pa·s]. |
| `mu_g` | Number | Yes | Vapor dynamic viscosity [Pa·s]. |
| `G` | Number | Yes | Mass flux G = ṁ/Aflow [kg/m²·s]. |
| `D` | Number | Yes | Diameter [m]. |
| `sigma` | Number | Yes | Surface tension [N/m]. |
