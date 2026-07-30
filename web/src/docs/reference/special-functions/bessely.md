---
name: bessely
category: Special Functions
summary: Bessel function of the second kind, order n — Y_n(x).
related: [besselj, besseli, besselk, bessely0, bessely1]
examples: []
tags: [special function, bessel, second kind, neumann, cylinder]
references:
  - "NIST Digital Library of Mathematical Functions, §10.2"
---

# bessely

Returns the **Bessel function of the second kind** `Y_n(x)` (Neumann function) of
integer order `n` — the second, singular-at-origin solution of Bessel's equation,
needed for problems with a hollow (non-zero inner radius) domain.

## Syntax

```
y = bessely(n, x)
```

## Description

`Y_n(x) → −∞` as `x → 0⁺`, so it appears only where the origin is excluded. For
fixed orders use `bessely0` / `bessely1`.

## Mathematical Formulation

`Y_n` is the second independent solution of Bessel's equation:

$$ x^2 y'' + x y' + (x^2 - n^2)y = 0, \qquad Y_n(x) = \frac{J_n(x)\cos(n\pi) - J_{-n}(x)}{\sin(n\pi)} $$

(taken as a limit for integer `n`), with the same recurrence as `J_n`.

> **Method:** standard library evaluation via the `J`/`Y` relations and recurrence.

## Examples

```
{ Y_0(x) -> -inf as x -> 0; finite for x > 0 }
y = bessely(0, 1)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `n` | Number | Yes | Integer order. |
| `x` | Number | Yes | Argument (> 0). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | Y_n(x). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `x ≤ 0` | `Y_n` is singular at and below 0; use a positive argument. |

## References

1. NIST *Digital Library of Mathematical Functions*, §10.2.
