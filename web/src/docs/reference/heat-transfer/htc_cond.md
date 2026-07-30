---
name: htc_cond
category: Heat Transfer
summary: In-tube condensation heat-transfer coefficient — Shah correlation.
related: [htc_evap, htc_1phase, dp_2phase]
examples: [ev-thermal-management]
tags: [heat transfer, condensation, two-phase, shah, refrigerant, film coefficient]
---

# htc_cond

Returns the **in-tube condensation heat-transfer coefficient** `h` [W/m²·K] for a
condensing two-phase refrigerant at quality `x`, using the **Shah**
correlation. Use it for the refrigerant side of a condenser or gas cooler.

## Syntax

```
h = htc_cond(fluid$, P, x, mdot, Dh, Aflow)
```

## Description

Condensation augments the liquid-only coefficient through the thinning liquid film
and vapor shear; Shah's correlation expresses this as a reduced-pressure and
quality-dependent enhancement.

## Mathematical Formulation

With the liquid-only coefficient $h_l$ (Dittus–Boelter), reduced pressure $p_r$,
and $Z = \big(\tfrac{1}{x}-1\big)^{0.8}p_r^{0.4}$,

$$ h_{TP} = h_l\left(1 + \frac{3.8}{Z^{0.95}}\right) $$

> **Method:** liquid-only `h_l` → Shah enhancement from `Z(x, p_r)` → `h_TP` at the
> local quality.

## Examples

### Example 1 — Condenser refrigerant-side film

[Run: ev-thermal-management]

**Expected:** a condensing film coefficient well above the single-phase liquid
value, decreasing as quality falls toward the subcooled liquid outlet.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `fluid$` | String | Yes | Refrigerant name. |
| `P` | Number | Yes | Pressure [Pa]. |
| `x` | Number | Yes | Vapor quality (0–1). |
| `mdot` | Number | Yes | Mass flow rate [kg/s]. |
| `Dh` | Number | Yes | Hydraulic diameter [m]. |
| `Aflow` | Number | Yes | Free-flow area [m²]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `h` | Number | Two-phase condensation coefficient [W/m²·K]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `x` outside [0, 1] | Quality must be a mass fraction in [0, 1]. |
