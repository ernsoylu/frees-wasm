---
name: tf2zp
category: Control Systems
summary: Transfer function to zero-pole-gain form.
related: [zp2tf, pole, zero, tf]
examples: []
tags: [control, zero pole gain, zpk, transfer function, factorization]
---

# tf2zp

Converts a transfer function `G(s) = num/den` to **zero-pole-gain** form: the zeros
(`zr`/`zi`), poles (`pr`/`pi`), and scalar gain `k`. It is the factored view of the
rational system, the inverse of `zp2tf`.

## Syntax

```
CALL tf2zp(num, den : zr, zi, pr, pi, k)
[zr, zi, pr, pi, k] = tf2zp(num, den)
```

## Mathematical Formulation

$$ G(s) = k\,\frac{\prod_i (s - z_i)}{\prod_j (s - p_j)} $$

where the zeros are the roots of `num`, the poles the roots of `den`, and `k` the
leading-coefficient ratio.

> **Method:** factor `num` and `den` (root-finding) and extract the gain.

## Examples

```
{ [zr,zi,pr,pi,k] = tf2zp(num, den) }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `num` | Vector | Yes | Numerator coefficients (descending powers of `s`). |
| `den` | Vector | Yes | Denominator coefficients. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `zr`, `zi` | Vector | Real / imaginary parts of the zeros. |
| `pr`, `pi` | Vector | Real / imaginary parts of the poles. |
| `k` | Number | Scalar gain. |
