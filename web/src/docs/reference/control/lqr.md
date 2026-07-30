---
name: lqr
category: Control Systems
summary: Linear-quadratic regulator optimal state-feedback gain.
related: [lqe, place, dare, pidtune]
examples: [controller-design-lqr-pid]
tags: [control, lqr, optimal, state feedback, riccati, regulator]
---

# lqr

Returns the **optimal state-feedback gain** `K` for a continuous linear-quadratic
regulator: the control `u = −Kx` that minimizes a weighted quadratic cost on the
states and the effort. Use it for systematic multi-state feedback design.

## Syntax

```
CALL lqr(A, B, Q, R : K)
K = lqr(A, B, Q, R)
```

## Description

`Q` (state weighting, ≥ 0) penalizes deviations; `R` (input weighting, > 0)
penalizes effort. Larger `Q/R` gives a faster, more aggressive regulator.

## Mathematical Formulation

Minimizing

$$ J = \int_0^\infty \big(\mathbf{x}^\top Q\,\mathbf{x} + \mathbf{u}^\top R\,\mathbf{u}\big)\,dt $$

gives `K = R⁻¹BᵀP`, where `P` solves the continuous algebraic Riccati equation:

$$ A^\top P + P A - P B R^{-1} B^\top P + Q = 0 $$

> **Method:** solve the continuous ARE for `P`, then `K = R⁻¹BᵀP`.

## Examples

### Example 1 — LQR gain for a plant

[Run: controller-design-lqr-pid]

**Expected:** a gain `K` placing the closed-loop poles `(A − BK)` for the chosen
`Q`, `R` trade-off.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Matrix | Yes | State matrix. |
| `B` | Matrix | Yes | Input matrix. |
| `Q` | Matrix | Yes | State weighting (symmetric, ≥ 0). |
| `R` | Matrix | Yes | Input weighting (symmetric, > 0). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `K` | Matrix | Optimal state-feedback gain (`u = −Kx`). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `NOT_STABILIZABLE` | `(A, B)` not stabilizable | The pair must be stabilizable for a solution to exist. |
| `R_NOT_PD` | `R` not positive-definite | Use a positive-definite input weighting. |
