---
name: void_zivi
category: Two-Phase Flow
summary: Zivi void fraction (slip S=(rho_l/rho_g)^(1/3))
related: []
examples: []
tags: [void, zivi, two, phase, flow]
---

# void_zivi

Zivi void fraction (slip S=(rho_l/rho_g)^(1/3))


## Syntax

```
void_zivi(x, rho_l, rho_g)
```

## Description

Returns the **Zivi void fraction**, using a slip ratio `S = (ρ_l/ρ_g)^{1/3}` from minimum-entropy-production — more realistic than the no-slip model.

## Mathematical Formulation

$$ \alpha = \frac{1}{1 + \frac{1-x}{x}\left(\frac{\rho_g}{\rho_l}\right)^{2/3}} \quad\text{(slip } S = (\rho_l/\rho_g)^{1/3}) $$

## Applicability

- **Where it applies:** The vapor fraction `α` for separated two-phase flow.
- **Valid when:** Moderate mass flux with appreciable slip; better than homogeneous, simpler than drift-flux.
- **How it's used:** Feeds mixture density / charge / static-head calculations.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Vapor quality (0–1). |
| `rho_l` | Number | Yes | Saturated-liquid density [kg/m³]. |
| `rho_g` | Number | Yes | Saturated-vapor density [kg/m³]. |
