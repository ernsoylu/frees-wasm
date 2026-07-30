---
name: void_rouhani
category: Two-Phase Flow
summary: Rouhani-Axelsson drift-flux void fraction (default)
related: []
examples: []
tags: [void, rouhani, two, phase, flow]
---

# void_rouhani

Rouhani-Axelsson drift-flux void fraction (default)


## Syntax

```
void_rouhani(x, rho_l, rho_g, G, sigma)
```

## Description

Returns the **Rouhani–Axelsson drift-flux void fraction** (the default) — it accounts for both phase slip and the radial distribution of vapor.

## Mathematical Formulation

$$ \alpha = \frac{x}{\rho_g}\left[(1 + 0.12(1-x))\left(\frac{x}{\rho_g} + \frac{1-x}{\rho_l}\right) + \frac{1.18(1-x)[g\sigma(\rho_l-\rho_g)]^{0.25}}{G\rho_l^{0.5}}\right]^{-1} $$

## Applicability

- **Where it applies:** The general-purpose vapor fraction `α` for refrigerant evaporators and condensers.
- **Valid when:** Recommended across flow regimes and mass fluxes; the default void model.
- **How it's used:** Feeds the refrigerant charge inventory, mixture density, and the gravitational pressure term.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Vapor quality (0–1). |
| `rho_l` | Number | Yes | Saturated-liquid density [kg/m³]. |
| `rho_g` | Number | Yes | Saturated-vapor density [kg/m³]. |
| `G` | Number | Yes | Mass flux G = ṁ/Aflow [kg/m²·s]. |
| `sigma` | Number | Yes | Surface tension [N/m]. |
