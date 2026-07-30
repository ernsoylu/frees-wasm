---
name: zp2tf
category: Control Systems
summary: Zero-pole-gain to transfer-function form.
related: [tf2zp, tf, pole, zero]
examples: []
tags: [control, zero pole gain, zpk, transfer function]
---

# zp2tf

Converts a **zero-pole-gain** description — zeros (`zr`/`zi`), poles (`pr`/`pi`),
and gain `k` — into a transfer function `num/den`. It is the inverse of
`tf2zp`, used to build a model from a factored (root) specification.

## Syntax

```
CALL zp2tf(zr, zi, pr, pi, k : num, den)
[num, den] = zp2tf(zr, zi, pr, pi, k)
```

## Mathematical Formulation

$$ G(s) = k\,\frac{\prod_i (s - z_i)}{\prod_j (s - p_j)} = \frac{\text{num}(s)}{\text{den}(s)} $$

> **Method:** expand the zero and pole factors into polynomials and scale by `k`.

## Examples

```
{ [num, den] = zp2tf(zr, zi, pr, pi, k) }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `zr`, `zi` | Vector | Yes | Real / imaginary parts of the zeros. |
| `pr`, `pi` | Vector | Yes | Real / imaginary parts of the poles. |
| `k` | Number | Yes | Scalar gain. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `num` | Vector | Numerator coefficients (descending powers of `s`). |
| `den` | Vector | Denominator coefficients. |
