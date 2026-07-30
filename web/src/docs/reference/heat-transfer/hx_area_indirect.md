---
name: hx_area_indirect
category: Heat Transfer
summary: Secondary (fin) surface area of a fin-and-tube heat exchanger.
related: [hx_area_direct, hx_fin_len, hx_eta_surf]
examples: [ev-thermal-management]
tags: [heat exchanger, geometry, secondary area, fin area, fin-and-tube]
---

# hx_area_indirect

Returns the **secondary surface area** [m²] — the total fin area — of a fin-and-tube
heat-exchanger air side, from the core width, tube count, and developed fin length.
With `hx_area_direct` it gives the primary/secondary area split
that `hx_eta_surf` weights by efficiency.

## Syntax

```
A_fin = hx_area_indirect(W, tubeCount, finLen)
```

## Description

The secondary (fin) area is the extended surface that operates below the base
temperature at efficiency `eta_fin < 1`. It typically dominates the total air-side
area, which is why the overall surface efficiency matters.

## Mathematical Formulation

The total fin area is the developed fin length (from
`hx_fin_len`) summed over the fins across the core width and tube
count (both fin faces):

$$ A_{\text{fin}} = f\big(W,\,\text{tubeCount},\,\text{finLen}\big) $$

(a fin-and-tube geometric construction).

> **Method:** direct geometric evaluation of the total fin area.

## Examples

### Example 1 — Fin area of an air-side core

[Run: ev-thermal-management]

**Expected:** the secondary area, usually the larger part of the air-side
`A_total = A_primary + A_fin`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `W` | Number | Yes | Core width [m]. |
| `tubeCount` | Number | Yes | Number of tubes. |
| `finLen` | Number | Yes | Developed fin length [m] (from `hx_fin_len`). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `A_fin` | Number | Secondary (fin) area [m²]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | A geometry input ≤ 0 | All dimensions must be positive. |
