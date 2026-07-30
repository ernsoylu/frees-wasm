---
name: ua_hx
category: Heat Transfer
summary: Overall heat-exchanger conductance UA from two side films and the wall.
related: [htc_1phase, htc_evap, htc_cond, hx_effectiveness]
examples: [ev-thermal-management]
tags: [heat exchanger, ua, overall conductance, thermal resistance, series]
---

# ua_hx

Returns the **overall thermal conductance** `UA` [W/K] of a two-stream heat
exchanger by combining the two convective side films and the wall as series
thermal resistances. Feed the result to `hx_effectiveness` /
`hx_NTU` to rate or size the exchanger.

## Syntax

```
UA = ua_hx(h1, A1, h2, A2, Rwall)
```

## Description

Each stream presents a film resistance `1/(h·A)`; the wall adds a conductive
resistance `Rwall`. In series these sum to the inverse conductance.

## Mathematical Formulation

$$ \frac{1}{UA} = \frac{1}{h_1 A_1} + R_{\text{wall}} + \frac{1}{h_2 A_2} $$

For finned (extended) surfaces each film term carries its overall surface
efficiency, `1/(η·h·A)` — supply the efficiency-weighted area or
an `h·η` product.

> **Method:** direct series-resistance sum; the smaller `h·A` dominates `UA`.

## Examples

### Example 1 — Chiller UA from refrigerant and coolant films

The EV thermal-management sizing forms each side's `h·A` (from the `htc_*`
correlations and the geometry) and combines them with the wall resistance.

[Run: ev-thermal-management]

**Expected:** `UA` is governed by the weaker side — typically the air/gas film,
whose `h·A` is far smaller than a boiling/condensing refrigerant side.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `h1` | Number | Yes | Side-1 film coefficient [W/m²·K]. |
| `A1` | Number | Yes | Side-1 (effective) area [m²]. |
| `h2` | Number | Yes | Side-2 film coefficient [W/m²·K]. |
| `A2` | Number | Yes | Side-2 (effective) area [m²]. |
| `Rwall` | Number | Yes | Wall conductive resistance [K/W]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `UA` | Number | Overall conductance [W/K]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | A film coefficient or area ≤ 0 | All resistances must be finite and positive. |
