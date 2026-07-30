---
name: besselj1
category: Special Functions
summary: Bessel function of the first kind, order 1 — J_1(x).
related: [besselj, besselj0, bessely1]
examples: []
tags: [special function, bessel, j1, first kind]
---

# besselj1

Returns `J_1(x)`, the **order-1 Bessel function of the first kind** — the
fixed-order specialization of `besselj`. `J_1(0) = 0`; it is the
derivative companion `J_0'(x) = −J_1(x)`.

## Syntax

```
y = besselj1(x)
```

## Mathematical Formulation

$$ J_1(x) = \sum_{k=0}^{\infty}\frac{(-1)^k}{k!\,(k+1)!}\left(\frac{x}{2}\right)^{2k+1}, \qquad J_0'(x) = -J_1(x) $$

## Examples

```
{ besselj1(0) = 0 }
y = besselj1(0)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Argument. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | J_1(x). |
