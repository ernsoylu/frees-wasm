---
name: void_homogeneous
category: Two-Phase Flow
summary: Homogeneous (no-slip) void fraction
related: []
examples: []
tags: [void, homogeneous, two, phase, flow]
---

# void_homogeneous

Homogeneous (no-slip) void fraction


## Syntax

```
void_homogeneous(x, rho_l, rho_g)
```

## Description

Returns the **homogeneous (no-slip) void fraction** — the vapor volume fraction assuming both phases move at the same velocity.

## Mathematical Formulation

$$ \alpha = \frac{1}{1 + \frac{1-x}{x}\frac{\rho_g}{\rho_l}} \quad\text{(no slip)} $$

## Applicability

- **Where it applies:** The vapor fraction `α` used in two-phase density, charge, and gravitational-head terms.
- **Valid when:** High-mass-flux / bubbly flow where slip is negligible; the simplest model (it overpredicts `α` at low mass flux).
- **How it's used:** Feeds the two-phase mixture density and `dp_gravity`. For better accuracy use `void_zivi` or `void_rouhani`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Vapor quality (0–1). |
| `rho_l` | Number | Yes | Saturated-liquid density [kg/m³]. |
| `rho_g` | Number | Yes | Saturated-vapor density [kg/m³]. |
