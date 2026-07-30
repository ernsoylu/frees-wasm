---
name: dp_2phase
category: Heat Transfer
summary: Two-phase frictional pressure drop — Lockhart-Martinelli / Chisholm multiplier.
related: [htc_evap, htc_cond, dp_1phase]
examples: [ev-thermal-management]
tags: [two-phase, pressure drop, lockhart-martinelli, chisholm, friction, refrigerant]
---

# dp_2phase

Returns the **two-phase frictional pressure drop** `dP` [Pa] of an evaporating or
condensing refrigerant over a passage length `L`, using the **Lockhart-Martinelli /
Chisholm** two-phase multiplier on the liquid-alone pressure gradient.

## Syntax

```
dP = dp_2phase(fluid$, P, x, mdot, Dh, Aflow, L)
```

## Description

Two-phase flow drops more pressure than the liquid alone because the vapor
accelerates and roughens the flow. The Chisholm multiplier scales the liquid-only
Darcy drop by a factor that depends on the Martinelli parameter `X`.

## Mathematical Formulation

With the liquid-only pressure gradient $(dP/dz)_l$ and the Martinelli parameter `X`,

$$ \phi_l^2 = 1 + \frac{C}{X} + \frac{1}{X^2}, \qquad \Delta P = \phi_l^2\left(\frac{dP}{dz}\right)_l L $$

where the Chisholm constant `C` ranges from 5 (laminar–laminar) to 20
(turbulent–turbulent).

> **Method:** liquid-only Darcy gradient × Chisholm multiplier `φ_l²(X, C)`,
> integrated over `L` at the local quality `x`.

## Examples

### Example 1 — Evaporator refrigerant-side pressure drop

[Run: ev-thermal-management]

**Expected:** a pressure drop several times the liquid-only value, rising with
quality as the vapor fraction grows.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `fluid$` | String | Yes | Refrigerant name. |
| `P` | Number | Yes | Pressure [Pa]. |
| `x` | Number | Yes | Vapor quality (0–1). |
| `mdot` | Number | Yes | Mass flow rate [kg/s]. |
| `Dh` | Number | Yes | Hydraulic diameter [m]. |
| `Aflow` | Number | Yes | Free-flow area [m²]. |
| `L` | Number | Yes | Passage length [m]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `dP` | Number | Two-phase frictional pressure drop [Pa]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `x` outside [0, 1] or `L ≤ 0` | Quality in [0, 1], positive length. |
