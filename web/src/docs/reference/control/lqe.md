---
name: lqe
category: Control Systems
summary: Linear-quadratic (Kalman) estimator gain.
related: [lqr, gram, balreal, obsv]
examples: [estimator-gramian-balreal]
tags: [control, kalman, estimator, observer, lqe, riccati]
---

# lqe

Returns the **optimal estimator (Kalman) gain** `L` for a linear system with process
noise (intensity via `G`, `Q`) and measurement noise (`R`). It is the dual of
`lqr`: the observer `x̂̇ = Ax̂ + Bu + L(y − Cx̂)` reconstructs the state from
noisy measurements with minimum error variance.

## Syntax

```
CALL lqe(A, G, C, Q, R : L)
L = lqe(A, G, C, Q, R)
```

## Description

`Q` is the process-noise covariance entering through `G`; `R` is the measurement-
noise covariance. The gain balances trust in the model against trust in the sensor.

## Mathematical Formulation

`L = PCᵀR⁻¹`, where `P` (the error covariance) solves the filter algebraic Riccati
equation (dual of the LQR ARE):

$$ A P + P A^\top - P C^\top R^{-1} C P + G Q G^\top = 0 $$

> **Method:** solve the filter ARE for `P`, then `L = PCᵀR⁻¹`.

## Examples

### Example 1 — Estimator gain for a plant

[Run: estimator-gramian-balreal]

**Expected:** an observer gain `L` that places the estimator poles `(A − LC)` for
the chosen noise weights.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Matrix | Yes | State matrix. |
| `G` | Matrix | Yes | Process-noise input matrix. |
| `C` | Matrix | Yes | Output matrix. |
| `Q` | Matrix | Yes | Process-noise covariance (≥ 0). |
| `R` | Matrix | Yes | Measurement-noise covariance (> 0). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `L` | Matrix | Optimal estimator gain. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `NOT_DETECTABLE` | `(A, C)` not detectable | The pair must be detectable for a solution to exist. |
