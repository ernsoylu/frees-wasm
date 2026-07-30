---
name: log2
category: Math
summary: Base-2 logarithm.
related: [ln, log10, exp]
examples: []
tags: [math, logarithm, base-2, binary, elementary]
references: []
---

# log2

Returns the **base-2 logarithm** `log₂(x)`. The argument must be a positive
dimensionless number. Common for information-theoretic quantities (bits) and
anything counted in powers of two.

## Syntax

```
y = log2(x)
```

## Description

A standard elementary function. For the natural log use `ln`; for base-10 use
`log10`. All three share the change-of-base identity $\log_2 x = \ln x / \ln 2$.

## Mathematical Formulation

$$ y = \log_2(x) = \frac{\ln x}{\ln 2}, \qquad x > 0 $$

> **Method:** evaluated as `ln(x) / ln(2)`; differentiated as
> $\frac{dy}{dx} = \frac{1}{x\,\ln 2}$ for Jacobians.

## Examples

### Example 1 — Bits needed to encode N states

```
{ Number of bits to address 1024 states }
N = 1024
bits = log2(N)       { 10 }
```

**Expected:** `bits = 10`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Positive dimensionless value. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | Base-2 logarithm of `x`. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `x ≤ 0` | The logarithm requires a positive argument. |
