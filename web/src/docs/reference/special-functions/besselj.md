---
name: besselj
category: Special Functions
summary: Bessel function of the first kind, order n — J_n(x).
related: [bessely, besseli, besselk, besselj0, besselj1]
examples: []
tags: [special function, bessel, first kind, cylinder, wave]
references:
  - "NIST Digital Library of Mathematical Functions, §10.2"
---

# besselj

Returns the **Bessel function of the first kind** `J_n(x)` of integer order `n`. It
is the finite-at-origin solution of Bessel's equation — the radial mode shape in
cylindrical wave, vibration, and diffusion problems.

## Syntax

```
y = besselj(n, x)
```

## Description

`J_n` oscillates with a slowly decaying amplitude. For the common fixed orders use
`besselj0` / `besselj1`.

## Mathematical Formulation

`J_n(x)` solves Bessel's equation and has the series

$$ x^2 y'' + x y' + (x^2 - n^2)y = 0, \qquad J_n(x) = \sum_{k=0}^{\infty} \frac{(-1)^k}{k!\,(n+k)!}\left(\frac{x}{2}\right)^{2k+n} $$

with the recurrence `J_{n-1}(x) + J_{n+1}(x) = (2n/x)J_n(x)`.

> **Method:** series for small `x`, asymptotic/recurrence for large `x`.

## Examples

```
{ besselj(0, 0) = 1 }
y = besselj(0, 0)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `n` | Number | Yes | Integer order. |
| `x` | Number | Yes | Argument. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | J_n(x). |

## References

1. NIST *Digital Library of Mathematical Functions*, §10.2.
