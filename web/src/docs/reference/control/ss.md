---
name: ss
category: Control Systems
summary: Create a state-space model from (A, B, C, D).
related: [ss2tf, tf2ss, ss2ss]
examples: []
tags: [control, state space, model, ss]
---

# ss

Builds a **state-space model** from the matrices `(A, B, C, D)` — the time-domain
representation `ẋ = Ax + Bu`, `y = Cx + Du` on which modern (state-feedback,
observer, LQR/LQE) design operates.

## Syntax

```
sys = ss(A, B, C, D)
```

## Mathematical Formulation

$$ \dot{\mathbf{x}} = A\mathbf{x} + B\mathbf{u}, \qquad \mathbf{y} = C\mathbf{x} + D\mathbf{u} $$

with transfer function `G(s) = C(sI − A)⁻¹B + D` (see `ss2tf`).

> **Method:** stores the `(A, B, C, D)` quadruple as a state-space model.

## Examples

```
{ sys = ss(A, B, C, D) }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Matrix | Yes | State matrix. |
| `B` | Matrix | Yes | Input matrix. |
| `C` | Matrix | Yes | Output matrix. |
| `D` | Number/Matrix | Yes | Feedthrough. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `sys` | State-space | The model `(A, B, C, D)`. |
