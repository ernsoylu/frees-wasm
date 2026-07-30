---
name: ss2ss
category: Control Systems
summary: Similarity transform of a state-space model, x = P·z.
related: [ss, balreal, ss2tf]
examples: []
tags: [control, similarity transform, state space, coordinate change]
---

# ss2ss

Applies a **similarity (coordinate) transform** `x = P·z` to a state-space model,
returning the equivalent realization `(An, Bn, Cn, Dn)`. The input-output behavior
is unchanged; only the state coordinates differ — used to reach canonical or
balanced forms.

## Syntax

```
CALL ss2ss(A, B, C, D, P : An, Bn, Cn, Dn)
[An, Bn, Cn, Dn] = ss2ss(A, B, C, D, P)
```

## Mathematical Formulation

With `x = P·z`:

$$ A_n = P^{-1}AP, \quad B_n = P^{-1}B, \quad C_n = CP, \quad D_n = D $$

The transfer function `C(sI−A)⁻¹B + D` is invariant under the transform.

> **Method:** apply the change of basis `P` to the quadruple.

## Examples

```
{ [An,Bn,Cn,Dn] = ss2ss(A,B,C,D,P) }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A`, `B`, `C`, `D` | Matrix | Yes | Original realization. |
| `P` | Matrix | Yes | Invertible transform (`x = P·z`). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `An`, `Bn`, `Cn`, `Dn` | Matrix | Transformed realization. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `SINGULAR_TRANSFORM` | `P` not invertible | Use a nonsingular transform matrix. |
