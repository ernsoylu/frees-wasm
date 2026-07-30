---
name: ss2tf
category: Control Systems
summary: Convert a state-space model (A, B, C, D) to a transfer function.
related: [tf2ss, series, feedback]
examples: [cruise-control]
tags: [control, state space, transfer function, ss2tf, model conversion]
---

# ss2tf

Converts a single-input single-output **state-space model** `(A, B, C, D)` into a
**transfer function** `G(s) = num(s)/den(s)`. Use it to move from a physically-built
state model to the frequency-domain form needed for classical loop design.

## Syntax

```
CALL ss2tf(A, B, C, D : num, den)
[num, den] = ss2tf(A, B, C, D)
```

## Description

`A` is the system matrix, `B` the input matrix, `C` the output matrix, and `D` the
feedthrough. The result is the rational transfer function relating the single input
to the single output.

## Mathematical Formulation

$$ G(s) = C\,(sI - A)^{-1} B + D = \frac{\text{num}(s)}{\text{den}(s)} $$

The denominator is the characteristic polynomial `den(s) = det(sI − A)`.

> **Method:** form `det(sI − A)` for `den` and the adjugate product for `num`.

## Examples

### Example 1 — Car velocity model

A 1000 kg car with viscous drag, output = velocity, converted to a transfer
function for PI cruise-control design.

[Run: cruise-control]

**Expected:** `G(s) = (1/m) / (s + c_drag/m) = 0.001/(s + 0.05)` — a first-order
velocity-from-force plant.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Matrix | Yes | System (state) matrix. |
| `B` | Vector | Yes | Input matrix/column. |
| `C` | Vector | Yes | Output matrix/row. |
| `D` | Number | Yes | Direct feedthrough term. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `num` | Vector | Numerator coefficients (descending powers of `s`). |
| `den` | Vector | Denominator coefficients (descending powers of `s`). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DIMENSION_MISMATCH` | `A`, `B`, `C`, `D` shapes inconsistent | `A` is n×n, `B` is n×1, `C` is 1×n, `D` is scalar (SISO). |
