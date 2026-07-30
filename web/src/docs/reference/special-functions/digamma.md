---
name: digamma
category: Special Functions
summary: Digamma function ψ(x) = d/dx ln Γ(x).
related: [gamma, loggamma, beta]
examples: []
tags: [special function, digamma, psi, gamma derivative]
references:
  - "NIST Digital Library of Mathematical Functions, §5.15"
---

# digamma

Returns the **digamma function** `ψ(x)` — the logarithmic derivative of the
`gamma` function. It arises in series summation, maximum-likelihood
estimation, and the derivatives of many special functions.

## Syntax

```
y = digamma(x)
```

## Description

Defined for all real `x` except the non-positive integers (poles). Satisfies a
recurrence that mirrors the Gamma recurrence.

## Mathematical Formulation

$$ \psi(x) = \frac{d}{dx}\ln\Gamma(x) = \frac{\Gamma'(x)}{\Gamma(x)} $$

with the recurrence and the harmonic-number link

$$ \psi(x+1) = \psi(x) + \frac{1}{x}, \qquad \psi(n) = -\gamma + \sum_{k=1}^{n-1}\frac{1}{k} $$

where `γ` is the Euler–Mascheroni constant.

> **Method:** recurrence to shift the argument upward, then an asymptotic series.

## Examples

```
{ psi(1) = -gamma (Euler-Mascheroni) ~ -0.5772 }
y = digamma(1)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Argument (not a non-positive integer). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | ψ(x). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `POLE` | `x` is 0 or a negative integer | ψ has poles there; use a non-integer argument. |

## References

1. NIST *Digital Library of Mathematical Functions*, §5.15.
