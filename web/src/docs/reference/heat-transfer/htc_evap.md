---
name: htc_evap
category: Heat Transfer
summary: Flow-boiling (evaporation) heat-transfer coefficient — Shah correlation.
related: [htc_cond, htc_1phase, dp_2phase]
examples: [ev-thermal-management]
tags: [heat transfer, boiling, evaporation, two-phase, shah, refrigerant, film coefficient]
---

# htc_evap

Returns the **flow-boiling heat-transfer coefficient** `h` [W/m²·K] for an
evaporating two-phase refrigerant at quality `x`, using the **Shah** correlation.
Use it for the refrigerant side of an evaporator or battery chiller.

## Syntax

```
h = htc_evap(fluid$, P, x, mdot, Dh, Aflow)
```

## Description

Flow boiling enhances heat transfer above the liquid-only value through convective
and nucleate-boiling mechanisms; the Shah correlation captures this as an
enhancement factor on the liquid-only coefficient.

## Mathematical Formulation

With the liquid-only coefficient $h_l$ (Dittus–Boelter on the liquid fraction) and
the convection number $Co = \big(\tfrac{1-x}{x}\big)^{0.8}(\rho_g/\rho_l)^{0.5}$,
the Shah convective-boiling enhancement is

$$ h_{TP} = h_l\,F_{cb}, \qquad F_{cb} = \frac{1.8}{Co^{0.8}} $$

with the nucleate-boiling branch taken where it dominates.

> **Method:** liquid-only `h_l` → Shah enhancement from `Co` (and boiling number) →
> `h_TP`, evaluated at the local quality `x`.

## Examples

### Example 1 — Evaporator refrigerant-side film

[Run: ev-thermal-management]

**Expected:** a boiling film coefficient several times the single-phase liquid
value — the high-conductance side of the chiller.

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
| `h` | Number | Two-phase boiling coefficient [W/m²·K]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `x` outside [0, 1] | Quality must be a mass fraction in [0, 1]. |
