---
name: hermiteh
category: Special Functions
summary: Hermite polynomial H_n(x) (physicists' convention).
related: [laguerrel, legendrep, chebyshevt]
examples: []
tags: [special function, hermite, orthogonal polynomial, gaussian]
references:
  - "NIST Digital Library of Mathematical Functions, §18.3"
---

# hermiteh

Returns the **Hermite polynomial** `H_n(x)` (physicists' convention) of degree `n`
— orthogonal on `(−∞, ∞)` with weight `e^{−x²}`, central to Gauss–Hermite
quadrature and the quantum harmonic oscillator.

## Syntax

```
y = hermiteh(n, x)
```

## Description

`H_0 = 1`, `H_1 = 2x`, with the standard three-term recurrence.

## Mathematical Formulation

$$ H_{n+1}(x) = 2x\,H_n(x) - 2n\,H_{n-1}(x), \qquad H_0 = 1,\ H_1 = 2x $$

with orthogonality $\int_{-\infty}^{\infty} H_m H_n\,e^{-x^2}\,dx = 2^n n!\sqrt{\pi}\,\delta_{mn}$.

> **Method:** three-term recurrence from `H_0`, `H_1`.

## Examples

```
{ H_2(x) = 4x^2 - 2; hermiteh(2, 1) = 2 }
y = hermiteh(2, 1)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `n` | Number | Yes | Polynomial degree (≥ 0). |
| `x` | Number | Yes | Argument. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | H_n(x). |

## References

1. NIST *Digital Library of Mathematical Functions*, §18.3.
