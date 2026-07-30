---
name: chebyshevt
category: Special Functions
summary: Chebyshev polynomial of the first kind, T_n(x).
related: [chebyshevu, legendrep]
examples: []
tags: [special function, chebyshev, orthogonal polynomial, approximation]
references:
  - "NIST Digital Library of Mathematical Functions, §18.3"
---

# chebyshevt

Returns the **Chebyshev polynomial of the first kind** `T_n(x)` of degree `n` — the
minimax-optimal polynomials on `[−1, 1]`, central to function approximation and
spectral methods.

## Syntax

```
y = chebyshevt(n, x)
```

## Description

On `[−1, 1]`, `T_n(x) = cos(n·arccos x)`, so `|T_n| ≤ 1`. `T_0 = 1`, `T_1 = x`.

## Mathematical Formulation

$$ T_n(\cos\theta) = \cos(n\theta), \qquad T_{n+1}(x) = 2x\,T_n(x) - T_{n-1}(x) $$

> **Method:** three-term recurrence from `T_0 = 1`, `T_1 = x`.

## Examples

```
{ T_2(x) = 2x^2 - 1; chebyshevt(2, 1) = 1 }
y = chebyshevt(2, 1)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `n` | Number | Yes | Polynomial degree (≥ 0). |
| `x` | Number | Yes | Argument. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | T_n(x). |

## References

1. NIST *Digital Library of Mathematical Functions*, §18.3.
