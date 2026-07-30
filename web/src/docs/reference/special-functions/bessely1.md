---
name: bessely1
category: Special Functions
summary: Bessel function of the second kind, order 1 — Y_1(x).
related: [bessely, bessely0, besselj1]
examples: []
tags: [special function, bessel, y1, second kind, neumann]
---

# bessely1

Returns `Y_1(x)`, the **order-1 Bessel function of the second kind** (Neumann) — the
fixed-order specialization of `bessely`. Singular as `x → 0⁺`.

## Syntax

```
y = bessely1(x)
```

## Mathematical Formulation

`Y_1` is the second independent order-1 solution of Bessel's equation,
with `Y_0'(x) = −Y_1(x)`.

## Examples

```
{ finite for x > 0 }
y = bessely1(1)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Argument (> 0). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | Y_1(x). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `x ≤ 0` | Singular at and below 0; use a positive argument. |
