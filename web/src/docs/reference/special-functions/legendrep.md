---
name: legendrep
category: Special Functions
summary: Legendre polynomial P_n(x).
related: [chebyshevt, hermiteh, laguerrel]
examples: []
tags: [special function, legendre, orthogonal polynomial, quadrature]
references:
  - "NIST Digital Library of Mathematical Functions, §18.3"
---

# legendrep

Returns the **Legendre polynomial** `P_n(x)` of degree `n` — the orthogonal
polynomials on `[−1, 1]` with unit weight, central to Gauss–Legendre quadrature and
spherical-harmonic expansions.

## Syntax

```
y = legendrep(n, x)
```

## Description

`P_0 = 1`, `P_1 = x`, and higher degrees follow Bonnet's recurrence. Orthogonal on
`[−1, 1]`.

## Mathematical Formulation

Bonnet recurrence (DLMF §18.9):

$$ (n+1)P_{n+1}(x) = (2n+1)\,x\,P_n(x) - n\,P_{n-1}(x), \qquad P_0 = 1,\ P_1 = x $$

with orthogonality $\int_{-1}^{1} P_m P_n\,dx = \tfrac{2}{2n+1}\delta_{mn}$.

> **Method:** three-term recurrence from `P_0`, `P_1`.

## Examples

```
{ P_2(x) = (3x^2 - 1)/2; legendrep(2, 1) = 1 }
y = legendrep(2, 1)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `n` | Number | Yes | Polynomial degree (≥ 0). |
| `x` | Number | Yes | Argument. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | P_n(x). |

## References

1. NIST *Digital Library of Mathematical Functions*, §18.3.
