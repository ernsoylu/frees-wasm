---
name: viewfactor_disks
category: Heat Transfer
summary: Diffuse radiation view factor between two coaxial parallel disks.
related: [viewfactor_plates, viewfactor_perp]
examples: [radiation-view-factors]
tags: [radiation, view factor, configuration factor, disks, coaxial, enclosure]
---

# viewfactor_disks

Returns the **diffuse radiation view factor** `F_{1→2}` from a disk of radius `r1`
to a coaxial parallel disk of radius `r2` separated by distance `L` — the fraction
of diffuse radiation leaving disk 1 that strikes disk 2. Use it in radiation
enclosure analysis instead of reading a chart.

## Syntax

```
F = viewfactor_disks(r1, r2, L)
```

## Description

For two directly-opposed coaxial disks, the view factor is a closed-form function
of the two radii and the separation. It feeds net radiation exchange via
`Q_{1→2} = A_1 F_{1→2} σ (T_1^4 − T_2^4)` (gray-diffuse, with the reciprocity and
summation rules closing the enclosure).

## Mathematical Formulation

With $R_1 = r_1/L$, $R_2 = r_2/L$, and $X = 1 + \dfrac{1 + R_2^2}{R_1^2}$,

$$ F_{1\to 2} = \frac{1}{2}\left\{\,X - \left[X^2 - 4\left(\frac{R_2}{R_1}\right)^2\right]^{1/2}\right\} $$

> **Method:** direct evaluation of the standard closed-form view-factor expression.

## Examples

### Example 1 — Coaxial disks

[Run: radiation-view-factors]

**Expected:** `viewfactor_disks(0.5, 1, 0.4) ≈ 0.83`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `r1` | Number | Yes | Radius of the emitting disk [m]. |
| `r2` | Number | Yes | Radius of the opposing coaxial disk [m]. |
| `L` | Number | Yes | Axial separation between the disks [m] (> 0). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `F` | Number | View factor F_{1→2} ∈ [0, 1]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `L ≤ 0` or a radius ≤ 0 | All three lengths must be positive. |
