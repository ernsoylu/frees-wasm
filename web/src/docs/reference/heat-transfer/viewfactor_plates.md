---
name: viewfactor_plates
category: Heat Transfer
summary: Diffuse radiation view factor between two aligned parallel rectangles.
related: [viewfactor_disks, viewfactor_perp]
examples: [radiation-view-factors]
tags: [radiation, view factor, configuration factor, parallel plates, rectangles]
---

# viewfactor_plates

Returns the **diffuse radiation view factor** `F_{1→2}` between two **aligned,
directly-opposed parallel rectangles** of side lengths `X` and `Y` separated by
distance `D`. Use it for parallel-surface radiation exchange without a chart lookup.

## Syntax

```
F = viewfactor_plates(X, Y, D)
```

## Description

Two identical, aligned, parallel rectangles see each other with a view factor that
depends only on the side-to-gap aspect ratios `x = X/D` and `y = Y/D`.

## Mathematical Formulation

With $x = X/D$ and $y = Y/D$,

$$ F_{1\to 2} = \frac{2}{\pi x y}\left\{ \ln\!\left[\frac{(1+x^2)(1+y^2)}{1+x^2+y^2}\right]^{1/2} + x\sqrt{1+y^2}\,\tan^{-1}\!\frac{x}{\sqrt{1+y^2}} + y\sqrt{1+x^2}\,\tan^{-1}\!\frac{y}{\sqrt{1+x^2}} - x\tan^{-1}x - y\tan^{-1}y \right\} $$

> **Method:** direct evaluation of the standard closed-form view-factor expression.

## Examples

### Example 1 — Aligned parallel rectangles

[Run: radiation-view-factors]

**Expected:** `viewfactor_plates(2, 2, 1) ≈ 0.41`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `X` | Number | Yes | First side length of the rectangles [m]. |
| `Y` | Number | Yes | Second side length of the rectangles [m]. |
| `D` | Number | Yes | Separation between the parallel planes [m] (> 0). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `F` | Number | View factor F_{1→2} ∈ [0, 1]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `D ≤ 0` or a side ≤ 0 | All three lengths must be positive. |
