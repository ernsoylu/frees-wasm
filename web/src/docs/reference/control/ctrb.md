---
name: ctrb
category: Control Systems
summary: Controllability matrix of a state-space pair (A, B).
related: [obsv, place, acker, gram]
examples: []
tags: [control, controllability, state space, rank]
---

# ctrb

Returns the **controllability matrix** `Co` of the pair `(A, B)`. The system is
controllable — every state reachable by the input, hence arbitrary pole placement
is possible — iff `Co` has full rank.

## Syntax

```
CALL ctrb(A, B : Co)
Co = ctrb(A, B)
```

## Mathematical Formulation

For an `n`-state system:

$$ \mathcal{C} = \begin{bmatrix} B & AB & A^2B & \cdots & A^{n-1}B \end{bmatrix} $$

The pair is controllable iff `rank(C) = n`.

> **Method:** assemble the Krylov block `[B, AB, …, Aⁿ⁻¹B]`.

## Examples

```
{ Co = ctrb(A, B); controllable iff rank(Co) = n }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Matrix | Yes | State matrix (n×n). |
| `B` | Matrix | Yes | Input matrix. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `Co` | Matrix | Controllability matrix. |
