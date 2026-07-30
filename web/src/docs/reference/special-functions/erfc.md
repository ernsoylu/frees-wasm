---
name: erfc
category: Special Functions
summary: Complementary error function erfc(x) = 1 − erf(x).
related: [erf, erfinv]
examples: []
tags: [special function, complementary error function, erfc, gaussian, tail]
references:
  - "NIST Digital Library of Mathematical Functions, §7.2"
---

# erfc

Returns the **complementary error function** `erfc(x) = 1 − erf(x)`. It is the
Gaussian tail probability and is computed directly (not as `1 − erf`) to preserve
precision for large `x`.

## Syntax

```
y = erfc(x)
```

## Description

Ranges from 2 (at `−∞`) to 0 (at `+∞`), with `erfc(0) = 1`. For large positive `x`
it is exponentially small, so the dedicated routine avoids catastrophic
cancellation.

## Mathematical Formulation

$$ \operatorname{erfc}(x) = 1 - \operatorname{erf}(x) = \frac{2}{\sqrt{\pi}}\int_x^\infty e^{-t^2}\,dt $$

> **Method:** direct rational/continued-fraction approximation of the tail.

## Examples

```
{ erfc(1) ~ 0.1573 }
y = erfc(1)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Real argument. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | erfc(x) ∈ (0, 2). |

## References

1. NIST *Digital Library of Mathematical Functions*, §7.2.
