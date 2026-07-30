---
name: besseli
category: Special Functions
summary: Modified Bessel function of the first kind, order n — I_n(x).
related: [besselk, besselj, besseli0, besseli1]
examples: []
tags: [special function, modified bessel, first kind, fin, conduction]
references:
  - "NIST Digital Library of Mathematical Functions, §10.25"
---

# besseli

Returns the **modified Bessel function of the first kind** `I_n(x)` of integer
order `n` — the finite-at-origin solution of the modified Bessel equation. It grows
exponentially and appears in fin conduction and cylindrical diffusion.

## Syntax

```
y = besseli(n, x)
```

## Description

Unlike `J_n`, `I_n` does not oscillate — it increases monotonically for `x > 0`.
For fixed orders use `besseli0` / `besseli1`.

## Mathematical Formulation

`I_n` solves the modified Bessel equation:

$$ x^2 y'' + x y' - (x^2 + n^2)y = 0, \qquad I_n(x) = \sum_{k=0}^{\infty}\frac{1}{k!\,(n+k)!}\left(\frac{x}{2}\right)^{2k+n} $$

with `I_n(x) = i^{-n}J_n(ix)`.

> **Method:** series for small `x`, asymptotic `I_n(x) ~ e^x/\sqrt{2\pi x}` for large `x`.

## Examples

```
{ besseli(0, 0) = 1 }
y = besseli(0, 0)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `n` | Number | Yes | Integer order. |
| `x` | Number | Yes | Argument. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | I_n(x). |

## References

1. NIST *Digital Library of Mathematical Functions*, §10.25.
