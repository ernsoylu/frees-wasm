---
name: ln
category: Math
summary: Natural logarithm (base e).
related: [exp, log10, log2]
examples: [karman-rocket]
tags: [math, logarithm, natural log, elementary]
references: []
---

# ln

Returns the **natural logarithm** `ln(x)` (base e), the inverse of `exp`.
The argument must be a positive dimensionless number.

## Syntax

```
y = ln(x)
```

## Description

A standard elementary function. For base-10 or base-2 use `log10` /
`log2`.

## Mathematical Formulation

$$ y = \ln(x) = \log_e(x), \qquad x > 0 $$

## Examples

### Example 1 — Rocket equation mass ratio

[Run: karman-rocket]

**Expected:** `ln` of the mass ratio gives the ideal velocity increment
(Tsiolkovsky).

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Positive dimensionless value. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | Natural logarithm of `x`. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `x ≤ 0` | The logarithm requires a positive argument. |
