---
name: dlyap
category: Control Systems
summary: Solve the discrete Lyapunov (Stein) equation A·X·Aᵀ − X + Q = 0.
related: [lyap, dare, dlqr]
examples: []
tags: [control, lyapunov, stein, discrete, stability]
---

# dlyap

Solves the **discrete Lyapunov (Stein) equation** for `X` — the discrete-time
counterpart of `lyap`. A positive-definite `X` for `Q > 0` certifies that
the discrete system `A` is Schur-stable (all eigenvalues inside the unit circle).

## Syntax

```
CALL dlyap(A, Q : X)
X = dlyap(A, Q)
```

## Mathematical Formulation

$$ A X A^\top - X + Q = 0 $$

For a Schur-stable `A` (`|λ_i(A)| < 1`) and `Q = Qᵀ ⪰ 0`, the unique solution is

$$ X = \sum_{k=0}^{\infty} A^k Q\,(A^\top)^k $$

> **Method:** Bartels–Stewart-type (Schur-based) solve of the Stein equation.

## Examples

```
{ X = dlyap(A, Q); X > 0 certifies A is Schur-stable when Q > 0 }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Matrix | Yes | Discrete state matrix. |
| `Q` | Matrix | Yes | Symmetric right-hand side. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `X` | Matrix | Symmetric solution. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `NO_UNIQUE_SOLUTION` | `λ_i·λ_j = 1` for some eigenvalue pair | The Stein operator is singular; check `A`'s spectrum. |
