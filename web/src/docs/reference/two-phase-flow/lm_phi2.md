---
name: lm_phi2
category: Two-Phase Flow
summary: Chisholm two-phase multiplier 1+C/X+1/X^2 on the liquid-alone drop
related: []
examples: []
tags: [lm, phi2, two, phase, flow]
---

# lm_phi2

Chisholm two-phase multiplier 1+C/X+1/X^2 on the liquid-alone drop


## Syntax

```
lm_phi2(X, C)
```

## Description

Returns the **Chisholm two-phase frictional multiplier** `φ_l² = 1 + C/X + 1/X²` on the liquid-alone pressure gradient — i.e. how much more pressure two-phase flow drops than the liquid flowing alone.

## Mathematical Formulation

$$ \phi_l^2 = 1 + \frac{C}{X} + \frac{1}{X^2} \quad\text{(Chisholm)} $$

## Applicability

- **Where it applies:** Two-phase frictional pressure drop in refrigerant evaporator/condenser passages.
- **Valid when:** Separated two-phase flow; the Chisholm constant `C` ranges 5 (laminar–laminar) to 20 (turbulent–turbulent).
- **How it's used:** Multiply the liquid-only Darcy gradient by `φ_l²` (with `lm_martinelli_tt` supplying `X`) to get the two-phase frictional `ΔP`. Friedel (`friedel_phi2`) is an alternative.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `X` | Number | Yes | Lockhart–Martinelli parameter. |
| `C` | Number | Yes | Empirical constant. |
