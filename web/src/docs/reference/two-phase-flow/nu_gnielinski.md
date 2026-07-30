---
name: nu_gnielinski
category: Two-Phase Flow
summary: Gnielinski single-phase Nusselt number
related: []
examples: []
tags: [nu, gnielinski, two, phase, flow]
---

# nu_gnielinski

Gnielinski single-phase Nusselt number


## Syntax

```
nu_gnielinski(Re, Pr)
```

## Description

Returns the **single-phase Nusselt number** by the Gnielinski correlation — more accurate than Dittus–Boelter, especially in the transitional-turbulent band.

## Mathematical Formulation

$$ Nu = \frac{(f/8)(Re-1000)Pr}{1 + 12.7\sqrt{f/8}\,(Pr^{2/3}-1)} $$

## Applicability

- **Where it applies:** Single-phase liquid or gas flow in a tube/channel (the preferred single-phase baseline).
- **Valid when:** Smooth tube, `3000 ≲ Re ≲ 5×10⁶`, `0.5 ≲ Pr ≲ 2000`; uses the Darcy friction factor.
- **How it's used:** The single-phase film coefficient (`h = Nu·k/D_h`) for coolant/oil/air lines, and the liquid-only baseline for two-phase correlations.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Re` | Number | Yes | Reynolds number. |
| `Pr` | Number | Yes | Prandtl number. |
