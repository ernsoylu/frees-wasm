---
name: loggamma
category: Special Functions
summary: Natural logarithm of the Gamma function, ln Γ(x) (overflow-safe).
related: [gamma, digamma, beta]
examples: []
tags: [special function, gamma, log gamma, overflow]
references:
  - "NIST Digital Library of Mathematical Functions, §5.2"
---

# loggamma

Returns **ln Γ(x)**, the natural logarithm of the `gamma` function. Use it
when `Γ(x)` itself would overflow (large `x`), or in likelihoods and combinatorial
ratios where the log domain is numerically safer.

## Syntax

```
y = loggamma(x)
```

## Description

For `x > 0`, `ln Γ(x)` grows only logarithmically faster than linearly, so it stays
finite far past where `Γ(x)` overflows double precision.

## Mathematical Formulation

$$ \ln\Gamma(x) = \ln\!\int_0^\infty t^{x-1}e^{-t}\,dt, \qquad \ln\Gamma(x+1) = \ln x + \ln\Gamma(x) $$

with the Stirling asymptotic

$$ \ln\Gamma(x) \sim \left(x-\tfrac12\right)\ln x - x + \tfrac12\ln(2\pi) + \frac{1}{12x} - \dots $$

> **Method:** Lanczos approximation of `ln Γ` directly (no intermediate overflow).

## Examples

```
{ loggamma(101) = ln(100!) }
y = loggamma(101)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Positive argument. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | ln Γ(x). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `x ≤ 0` | Use a positive argument. |

## References

1. NIST *Digital Library of Mathematical Functions*, §5.2.
