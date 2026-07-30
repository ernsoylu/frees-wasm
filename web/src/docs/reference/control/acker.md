---
name: acker
category: Control Systems
summary: Single-input pole placement via Ackermann's formula.
related: [place, ctrb, lqr]
examples: []
tags: [control, pole placement, ackermann, state feedback]
---

# acker

Returns the **state-feedback gain** `K` for a single-input system using
**Ackermann's formula**, placing the closed-loop poles of `(A, B)` at the desired
locations `pr ± j·pi`. It is the explicit closed-form counterpart of
`place`.

## Syntax

```
CALL acker(A, B, pr, pi : K)
K = acker(A, B, pr, pi)
```

## Description

Ackermann's formula gives `K` directly from the desired characteristic polynomial
and the controllability matrix; it is exact for single-input systems but
numerically sensitive for high order.

## Mathematical Formulation

With desired characteristic polynomial `Φ(s) = Π(s − p_i)` and controllability
matrix `C = [B AB … Aⁿ⁻¹B]`:

$$ K = \begin{bmatrix} 0 & \cdots & 0 & 1 \end{bmatrix}\,\mathcal{C}^{-1}\,\Phi(A) $$

> **Method:** Ackermann's formula evaluated from `Φ(A)` and `ctrb(A, B)`.

## Examples

```
{ K = acker(A, B, [-2,-2], [1,-1]) for a single-input controllable plant }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Matrix | Yes | State matrix. |
| `B` | Vector | Yes | Single-input matrix. |
| `pr` | Vector | Yes | Real parts of the desired poles. |
| `pi` | Vector | Yes | Imaginary parts of the desired poles. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `K` | Vector | State-feedback gain (`u = −Kx`). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `NOT_CONTROLLABLE` | `ctrb(A, B)` singular | Ackermann needs a controllable single-input pair. |
