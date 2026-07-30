---
name: laguerrel
category: Special Functions
summary: Laguerre polynomial L_n(x).
related: [hermiteh, legendrep, chebyshevt]
examples: []
tags: [special function, laguerre, orthogonal polynomial, quadrature]
references:
  - "NIST Digital Library of Mathematical Functions, §18.3"
---

# laguerrel

Returns the **Laguerre polynomial** `L_n(x)` of degree `n` — orthogonal on
`[0, ∞)` with weight `e^{−x}`, central to Gauss–Laguerre quadrature and the radial
hydrogen wavefunctions.

## Syntax

```
y = laguerrel(n, x)
```

## Description

`L_0 = 1`, `L_1 = 1 − x`, with the standard three-term recurrence.

## Mathematical Formulation

$$ (n+1)L_{n+1}(x) = (2n+1-x)L_n(x) - n\,L_{n-1}(x), \qquad L_0 = 1,\ L_1 = 1 - x $$

with orthogonality $\int_0^\infty L_m L_n\,e^{-x}\,dx = \delta_{mn}$.

> **Method:** three-term recurrence from `L_0`, `L_1`.

## Examples

```
{ L_1(x) = 1 - x; laguerrel(1, 0) = 1 }
y = laguerrel(1, 0)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `n` | Number | Yes | Polynomial degree (≥ 0). |
| `x` | Number | Yes | Argument. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | L_n(x). |

## References

1. NIST *Digital Library of Mathematical Functions*, §18.3.
