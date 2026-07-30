---
name: dlqr
category: Control Systems
summary: Discrete-time LQR optimal state-feedback gain (via the DARE).
related: [lqr, dare, place]
examples: []
tags: [control, lqr, discrete, optimal, state feedback, riccati]
---

# dlqr

Returns the **discrete-time LQR gain** `K` — the discrete counterpart of
`lqr`. The control `u_k = −K x_k` minimizes a summed quadratic cost on the
states and effort.

## Syntax

```
CALL dlqr(A, B, Q, R : K)
K = dlqr(A, B, Q, R)
```

## Mathematical Formulation

Minimizing `J = Σ (xₖᵀQxₖ + uₖᵀRuₖ)` gives

$$ K = (R + B^\top X B)^{-1} B^\top X A $$

where `X` is the stabilizing solution of the discrete algebraic Riccati equation
(`dare`).

> **Method:** solve the DARE for `X`, then form `K`.

## Examples

```
{ K = dlqr(A, B, Q, R); discrete optimal regulator gain }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Matrix | Yes | Discrete state matrix. |
| `B` | Matrix | Yes | Input matrix. |
| `Q` | Matrix | Yes | State weight (⪰ 0). |
| `R` | Matrix | Yes | Input weight (≻ 0). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `K` | Matrix | Discrete state-feedback gain (`u = −Kx`). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `NOT_STABILIZABLE` | `(A, B)` not stabilizable | The pair must be stabilizable. |
