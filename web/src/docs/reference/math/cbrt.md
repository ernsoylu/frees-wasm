---
name: cbrt
category: Math
summary: Cube root of a number (with unit propagation).
related: [sqrt, exp, abs]
examples: []
tags: [math, cube root, elementary]
references: []
---

# cbrt

Returns the **cube root** `∛x`. Unlike `sqrt`, it is defined for negative
arguments (`cbrt(-8) = -2`). In frees it also propagates units — the result
carries one-third of the argument's dimension (e.g. `cbrt(m^3) → m`).

## Syntax

```
y = cbrt(x)
```

## Description

A standard elementary function, equivalent to `x^(1/3)` but valid for negative
`x` as well (the real cube root). Reach for it for characteristic lengths from a
volume, or any relation that inverts a cube.

## Mathematical Formulation

$$ y = \sqrt[3]{x} = x^{1/3} $$

> **Method:** direct evaluation via the platform `cbrt` (real-valued for all `x`);
> the solver differentiates it as $\frac{dy}{dx} = \tfrac{1}{3}x^{-2/3}$ for Jacobians.

## Examples

### Example 1 — Characteristic length of a cubic volume

```
{ Edge length of a cube from its volume }
Vol = 0.027 [m^3]
L = cbrt(Vol)        { 0.3 m }
```

**Expected:** `L = 0.3 [m]` — note the unit reduces from `m^3` to `m`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Any real value (any unit); negatives are allowed. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | The cube root, with one-third of the argument's dimension. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DIMENSION_ERROR` | Argument dimension is not a perfect cube of a representable unit | Ensure the argument's units divide evenly by 3 (e.g. `m^3`, `m^6`). |
