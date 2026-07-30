---
name: besselk0
category: Special Functions
summary: Modified Bessel function of the second kind, order 0 — K_0(x).
related: [besselk, besselk1, besseli0]
examples: []
tags: [special function, modified bessel, k0, second kind, macdonald]
---

# besselk0

Returns `K_0(x)`, the **order-0 modified Bessel function of the second kind**
(Macdonald) — the fixed-order specialization of `besselk`. Singular as
`x → 0⁺`, decaying like `e^{−x}`.

## Syntax

```
y = besselk0(x)
```

## Mathematical Formulation

`K_0` is the decaying order-0 solution of the modified Bessel equation, with
`K_0(x) ~ √(π/2x)·e^{−x}` for large `x`.

## Examples

```
{ decays for large x }
y = besselk0(1)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Argument (> 0). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | K_0(x). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `x ≤ 0` | Singular at and below 0; use a positive argument. |
