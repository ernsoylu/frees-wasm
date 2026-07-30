---
name: zero
category: Control Systems
summary: Zeros of a transfer function (roots of the numerator).
related: [pole, margin]
examples: [control-analysis-report]
tags: [control, zeros, transfer function, root locus]
---

# zero

Returns the **zeros** of a transfer function `G(s) = num(s)/den(s)` — the roots of
its numerator — split into real (`zr`) and imaginary (`zi`) parts. Zeros shape the
transient response and the root-locus departure, and a right-half-plane zero
signals non-minimum-phase behavior.

## Syntax

```
CALL zero(num, den : zr, zi)
[zr, zi] = zero(num, den)
```

## Description

Zeros are the values of `s` that make `G(s) = 0`. They do not affect stability
(that is the poles) but strongly influence overshoot, undershoot, and how a root
locus bends.

## Mathematical Formulation

$$ \text{num}(s) = 0 \quad\Longrightarrow\quad s = z_k $$

> **Method:** numerical polynomial root-finding on `num`.

## Examples

### Example 1 — Zero of a second-order plant

For `G(s) = (s + 2)/(s² + 4s + 25)`:

[Run: control-analysis-report]

**Expected:** a single real zero at `s = −2` (`zr = −2`, `zi = 0`).

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `num` | Vector | Yes | Numerator coefficients (descending powers of `s`). |
| `den` | Vector | Yes | Denominator coefficients (descending powers of `s`). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `zr` | Vector | Real parts of the zeros. |
| `zi` | Vector | Imaginary parts of the zeros. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `EMPTY_NUMERATOR` | `num` is constant or empty | A constant numerator has no finite zeros. |
