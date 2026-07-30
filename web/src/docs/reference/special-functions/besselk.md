---
name: besselk
category: Special Functions
summary: Modified Bessel function of the second kind, order n — K_n(x).
related: [besseli, bessely, besselk0, besselk1]
examples: []
tags: [special function, modified bessel, second kind, macdonald, decay]
references:
  - "NIST Digital Library of Mathematical Functions, §10.25"
---

# besselk

Returns the **modified Bessel function of the second kind** `K_n(x)` (Macdonald
function) of integer order `n` — the exponentially decaying solution of the modified
Bessel equation, used for outward (decaying) cylindrical fields.

## Syntax

```
y = besselk(n, x)
```

## Description

`K_n(x) → ∞` as `x → 0⁺` and decays like `e^{−x}` for large `x`. The natural
partner to `besseli`. For fixed orders use `besselk0` /
`besselk1`.

## Mathematical Formulation

`K_n` is the decaying solution of the modified Bessel equation:

$$ x^2 y'' + x y' - (x^2 + n^2)y = 0, \qquad K_n(x) = \frac{\pi}{2}\frac{I_{-n}(x) - I_n(x)}{\sin(n\pi)} $$

(a limit for integer `n`), with asymptotic `K_n(x) ~ \sqrt{\pi/2x}\,e^{-x}`.

> **Method:** standard library evaluation via the `I`/`K` relations.

## Examples

```
{ K_0(x) -> inf as x -> 0; decays for large x }
y = besselk(0, 1)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `n` | Number | Yes | Integer order. |
| `x` | Number | Yes | Argument (> 0). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | K_n(x). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `x ≤ 0` | `K_n` is singular at and below 0; use a positive argument. |

## References

1. NIST *Digital Library of Mathematical Functions*, §10.25.
