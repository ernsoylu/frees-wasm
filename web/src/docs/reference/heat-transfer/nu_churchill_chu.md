---
name: nu_churchill_chu
category: Heat Transfer
summary: Nu, free convection from Rayleigh. SIDE: natural convection (still air / quiescent fluid). HX: passive/low-flow surfaces
related: []
examples: []
tags: [nu, churchill, chu, heat, transfer]
---

# nu_churchill_chu

Nu, free convection from Rayleigh. SIDE: natural convection (still air / quiescent fluid). HX: passive/low-flow surfaces


## Syntax

```
nu_churchill_chu(Ra, Pr)
```

## Description

Nu, free convection from Rayleigh. SIDE: natural convection (still air / quiescent fluid). HX: passive/low-flow surfaces

## Mathematical Formulation

$$ Nu = \left\{0.60 + \frac{0.387\,Ra^{1/6}}{[1 + (0.559/Pr)^{9/16}]^{8/27}}\right\}^2 \quad\text{(Churchill–Chu)} $$

## Applicability

- **Where it applies:** Natural convection from a surface to still air / quiescent fluid.
- **Valid when:** Free (buoyancy-driven) convection, characterized by the Rayleigh number.
- **How it's used:** Passive/low-flow surfaces; blend with a forced-convection `Nu` via `nu_blend`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Ra` | Number | Yes | Rayleigh number. |
| `Pr` | Number | Yes | Prandtl number. |
