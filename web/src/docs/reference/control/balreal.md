---
name: balreal
category: Control Systems
summary: Internally-balanced state-space realization for model reduction.
related: [gram, ctrb, obsv, ss2tf]
examples: [estimator-gramian-balreal]
tags: [control, balanced realization, model reduction, gramian, hankel]
---

# balreal

Returns an **internally-balanced realization** `(Ab, Bb, Cb)` of a state-space
system — a coordinate transform in which the controllability and observability
Gramians are equal and diagonal (the Hankel singular values). States with small
Hankel values can then be truncated for model reduction.

## Syntax

```
CALL balreal(A, B, C : Ab, Bb, Cb)
[Ab, Bb, Cb] = balreal(A, B, C)
```

## Description

Balancing ranks the state directions by their joint input-output energy, so a
reduced model that drops the least-significant states keeps the dominant dynamics.

## Mathematical Formulation

Find `T` such that the transformed Gramians satisfy:

$$ \tilde W_c = \tilde W_o = \Sigma = \mathrm{diag}(\sigma_1 \ge \sigma_2 \ge \dots), \qquad (A_b, B_b, C_b) = (T^{-1}AT,\ T^{-1}B,\ CT) $$

where the `σ_i` are the Hankel singular values.

> **Method:** compute the Gramians, form the balancing transform from their joint
> eigenstructure, and apply it.

## Examples

### Example 1 — Balanced realization of a plant

[Run: estimator-gramian-balreal]

**Expected:** a realization whose equal, diagonal Gramians expose the Hankel
singular values for truncation.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Matrix | Yes | State matrix (stable). |
| `B` | Matrix | Yes | Input matrix. |
| `C` | Matrix | Yes | Output matrix. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `Ab` | Matrix | Balanced state matrix. |
| `Bb` | Matrix | Balanced input matrix. |
| `Cb` | Matrix | Balanced output matrix. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `NOT_MINIMAL` | system not controllable/observable | Balancing requires a minimal (or stable) realization. |
