---
name: lyap
category: Control Systems
summary: Solve the continuous Lyapunov equation A·X + X·Aᵀ + Q = 0.
related: [dlyap, dare, gram]
examples: []
tags: [control, lyapunov, stability, gramian, riccati]
---

# lyap

Solves the **continuous Lyapunov equation** for `X`. It underpins stability
analysis (a positive-definite `X` for `Q > 0` certifies stability of `A`) and the
controllability/observability Gramians.

## Syntax

```
CALL lyap(A, Q : X)
X = lyap(A, Q)
```

## Mathematical Formulation

$$ A X + X A^\top + Q = 0 $$

For a Hurwitz `A` and `Q = Qᵀ ⪰ 0`, the unique solution is

$$ X = \int_0^\infty e^{A t} Q\, e^{A^\top t}\,dt $$

> **Method:** Bartels–Stewart (Schur-based) solve of the linear Lyapunov system.

## Examples

```
{ X = lyap(A, Q); X > 0 certifies A is stable when Q > 0 }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Matrix | Yes | State matrix (stable for a bounded solution). |
| `Q` | Matrix | Yes | Symmetric right-hand side. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `X` | Matrix | Symmetric solution. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `NO_UNIQUE_SOLUTION` | `A` shares eigenvalues with `−Aᵀ` | The Lyapunov operator is singular; check `A`'s spectrum. |
