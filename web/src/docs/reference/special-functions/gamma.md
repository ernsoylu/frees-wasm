---
name: gamma
category: Special Functions
summary: Gamma function Γ(x), the continuous extension of the factorial.
related: [loggamma, digamma, beta, factorial]
examples: []
tags: [special function, gamma, factorial, euler]
references:
  - "NIST Digital Library of Mathematical Functions, §5.2 (dlmf.nist.gov)"
---

# gamma

Returns the **Gamma function** `Γ(x)` — the continuous extension of the factorial,
with `Γ(n+1) = n!` for non-negative integers. It appears throughout probability,
combinatorics, and special-function identities.

## Syntax

```
y = gamma(x)
```

## Description

Defined for all real `x` except the non-positive integers (where it has poles).
For large arguments use `loggamma` to avoid overflow.

## Mathematical Formulation

$$ \Gamma(x) = \int_0^\infty t^{x-1} e^{-t}\,dt, \qquad x > 0 $$

with the recurrence and factorial link

$$ \Gamma(x+1) = x\,\Gamma(x), \qquad \Gamma(n+1) = n! $$

> **Method:** Lanczos / Stirling approximation evaluated to machine precision.

## Examples

```
{ Gamma(5) = 4! = 24 }
y = gamma(5)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Argument (not a non-positive integer). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | Γ(x). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `POLE` | `x` is 0 or a negative integer | Γ has poles there; use a non-integer or positive argument. |
| `OVERFLOW` | `x` large | Use `loggamma(x)` and work in the log domain. |

## References

1. NIST *Digital Library of Mathematical Functions*, §5.2.
