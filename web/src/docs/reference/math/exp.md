---
name: exp
category: Math
summary: Exponential function e raised to the power x.
related: [ln, log10, sqrt]
examples: [damped-oscillator, sounding-rocket-trajectory]
tags: [math, exponential, elementary]
references: []
---

# exp

Returns the **exponential** `eˣ`. The argument must be dimensionless (the function
is only meaningful for a pure number).

## Syntax

```
y = exp(x)
```

## Description

A standard elementary function, the inverse of `ln`. It appears throughout
transient and decay models.

## Mathematical Formulation

$$ y = e^{x}, \qquad e \approx 2.71828 $$

## Examples

### Example 1 — Decay envelope of a damped oscillator

[Run: damped-oscillator]

**Expected:** `exp` of the negative-rate·time term gives the decaying amplitude.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Dimensionless exponent. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | `eˣ` (dimensionless). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `UNIT_MISMATCH` | `x` carries a unit | The exponent must be dimensionless. |
