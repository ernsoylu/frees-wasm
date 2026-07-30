---
name: place
category: Control Systems
summary: State-feedback pole placement to a set of desired closed-loop poles.
related: [acker, lqr, ctrb]
examples: []
tags: [control, pole placement, state feedback, ackermann, design]
---

# place

Returns the **state-feedback gain** `K` that moves the closed-loop poles of
`(A, B)` to the desired locations given by their real/imaginary parts (`pr`, `pi`).
The control law `u = −Kx` makes `eig(A − BK)` equal the requested poles.

## Syntax

```
CALL place(A, B, pr, pi : K)
K = place(A, B, pr, pi)
```

## Description

The pair `(A, B)` must be controllable for arbitrary pole placement. Provide the
desired poles as conjugate pairs in `pr ± j·pi`.

## Mathematical Formulation

Find `K` such that

$$ \det\!\big(sI - (A - BK)\big) = \prod_i (s - p_i) $$

with the desired characteristic polynomial set by `{p_i}`.

> **Method:** solve for `K` from the desired characteristic polynomial (Ackermann /
> robust placement); see `acker` for the single-input Ackermann form.

## Examples

```
{ place the regulator poles of a controllable (A,B) at -2 +/- j }
{ K = place(A, B, [-2,-2], [1,-1]) }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Matrix | Yes | State matrix. |
| `B` | Matrix | Yes | Input matrix. |
| `pr` | Vector | Yes | Real parts of the desired poles. |
| `pi` | Vector | Yes | Imaginary parts of the desired poles. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `K` | Matrix | State-feedback gain (`u = −Kx`). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `NOT_CONTROLLABLE` | `(A, B)` not controllable | Arbitrary placement needs a controllable pair (check `ctrb`). |
