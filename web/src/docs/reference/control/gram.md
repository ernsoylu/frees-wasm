---
name: gram
category: Control Systems
summary: Controllability or observability Gramian of a state-space system.
related: [ctrb, obsv, balreal]
examples: [estimator-gramian-balreal]
tags: [control, gramian, controllability, observability, balanced]
---

# gram

Returns the **controllability** (`'c'`) or **observability** (`'o'`) Gramian of a
stable state-space system. The Gramians quantify how strongly each state direction
is excited by the input / observed at the output, and underpin balanced model
reduction (`balreal`).

## Syntax

```
CALL gram(A, M, 'c' : W)
W = gram(A, M, 'o')
```

## Description

For `type$ = 'c'`, `M = B` and `W` is the controllability Gramian; for `'o'`,
`M = C` and `W` is the observability Gramian. Both are symmetric positive-definite
for a stable, controllable/observable system.

## Mathematical Formulation

The Gramians solve the Lyapunov equations:

$$ A W_c + W_c A^\top + B B^\top = 0, \qquad A^\top W_o + W_o A + C^\top C = 0 $$

equivalently $W_c = \int_0^\infty e^{A t} B B^\top e^{A^\top t}\,dt$.

> **Method:** solve the appropriate Lyapunov equation for `W`.

## Examples

### Example 1 — Gramian of a plant

[Run: estimator-gramian-balreal]

**Expected:** a symmetric positive-definite Gramian whose eigenstructure ranks the
state directions by controllability/observability.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Matrix | Yes | State matrix (must be stable). |
| `M` | Matrix | Yes | `B` for controllability, `C` for observability. |
| `type$` | String | Yes | `'c'` (controllability) or `'o'` (observability). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `W` | Matrix | The requested Gramian (symmetric). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `NOT_STABLE` | `A` has non-negative eigenvalues | The Gramian integral converges only for a stable `A`. |
