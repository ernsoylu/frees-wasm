---
name: besselk1
category: Special Functions
summary: Modified Bessel function of the second kind, order 1 — K_1(x).
related: [besselk, besselk0, besseli1]
examples: []
tags: [special function, modified bessel, k1, second kind, macdonald]
---

# besselk1

Returns `K_1(x)`, the **order-1 modified Bessel function of the second kind**
(Macdonald) — the fixed-order specialization of `besselk`. Singular as
`x → 0⁺`, decaying like `e^{−x}`, with `K_0'(x) = −K_1(x)`.

## Syntax

```
y = besselk1(x)
```

## Mathematical Formulation

`K_1` is the decaying order-1 solution of the modified Bessel equation, with
`K_0'(x) = −K_1(x)` and `K_1(x) ~ √(π/2x)·e^{−x}` for large `x`.

## Examples

```
{ decays for large x }
y = besselk1(1)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Argument (> 0). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | K_1(x). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `x ≤ 0` | Singular at and below 0; use a positive argument. |
