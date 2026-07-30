---
name: nu_cavallini_zecchin
category: Two-Phase Flow
summary: Cavallini-Zecchin condensation Nusselt number
related: []
examples: []
tags: [nu, cavallini, zecchin, two, phase, flow]
---

# nu_cavallini_zecchin

Cavallini-Zecchin condensation Nusselt number


## Syntax

```
nu_cavallini_zecchin(Re_l, Pr_l, x, rho_l, rho_g)
```

## Description

Returns the **in-tube condensation Nusselt number** by the Cavallini–Zecchin correlation; the condensing-side film coefficient follows as `h = Nu·k_l/D_h`. It is one of the standard shear-dominated condensation correlations.

## Mathematical Formulation

$$ Nu = 0.05\,Re_{eq}^{0.8}\,Pr_l^{0.33} \quad\text{(Cavallini–Zecchin condensation)} $$

## Applicability

- **Where it applies:** The condensing two-phase refrigerant **inside the tubes of a condenser or gas-cooler**.
- **Valid when:** Annular, vapor-shear-controlled in-tube condensation with a turbulent liquid film; evaluate at the local vapor quality `x` (integrate across the pass for a mean value).
- **How it's used:** Convert to a film coefficient `h = Nu·k_l/D_h`, then combine it with the air/coolant side and the wall via `ua_hx`. Alternatives: `nu_shah` (broader range) and `nu_traviss`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Re_l` | Number | Yes | Liquid-only Reynolds number. |
| `Pr_l` | Number | Yes | Liquid Prandtl number. |
| `x` | Number | Yes | Vapor quality (0–1). |
| `rho_l` | Number | Yes | Saturated-liquid density [kg/m³]. |
| `rho_g` | Number | Yes | Saturated-vapor density [kg/m³]. |
