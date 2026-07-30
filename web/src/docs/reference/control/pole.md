---
name: pole
category: Control Systems
summary: Poles of a transfer function (roots of the denominator).
related: [zero, margin, residue]
examples: [control-analysis-report]
tags: [control, poles, stability, transfer function, eigenvalues]
---

# pole

Returns the **poles** of a transfer function `G(s) = num(s)/den(s)` — the roots of
its denominator — split into real (`pr`) and imaginary (`pi`) parts. The poles set
the natural modes and stability: a continuous-time system is stable iff every pole
has a negative real part.

## Syntax

```
CALL pole(num, den : pr, pi)
[pr, pi] = pole(num, den)
```

## Description

`num` and `den` are coefficient vectors in descending powers of `s`. The poles
govern the transient response (decay rates and oscillation frequencies); their
location in the s-plane is the primary stability indicator.

## Mathematical Formulation

The poles are the roots of the characteristic (denominator) polynomial,

$$ \text{den}(s) = 0 \quad\Longrightarrow\quad s = p_k = \sigma_k \pm j\omega_k $$

A complex pair $\sigma \pm j\omega$ corresponds to natural frequency
$\omega_n = \sqrt{\sigma^2 + \omega^2}$ and damping ratio $\zeta = -\sigma/\omega_n$.

> **Method:** numerical polynomial root-finding on `den`.

## Examples

### Example 1 — Poles of an underdamped second-order plant

For `G(s) = (s + 2)/(s² + 4s + 25)`:

[Run: control-analysis-report]

**Expected:** poles `s = −2 ± 4.583j` (`pr = −2`, `pi = ±4.583`); both in the
left half-plane, so the plant is stable (`ω_n = 5 rad/s`, `ζ = 0.4`).

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `num` | Vector | Yes | Numerator coefficients (descending powers of `s`). |
| `den` | Vector | Yes | Denominator coefficients (descending powers of `s`). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `pr` | Vector | Real parts of the poles. |
| `pi` | Vector | Imaginary parts of the poles. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `EMPTY_DENOMINATOR` | `den` has no nonzero leading coefficient | Provide a valid denominator polynomial. |
