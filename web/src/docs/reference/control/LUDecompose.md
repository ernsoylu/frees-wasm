---
name: LUDecompose
category: Matrix
summary: LU decomposition of a matrix (A = L·U).
related: [SolveLinear, Inverse, Determinant]
examples: []
tags: [matrix, lu decomposition, factorization, linear solve]
---

# LUDecompose

Returns the **LU decomposition** of a square matrix `A` — a lower-triangular `L`
and upper-triangular `U` whose product is `A` (with partial pivoting). It is the
workhorse factorization behind linear solves and determinants.

## Syntax

```
CALL LUDecompose(A : L, U)
[L, U] = LUDecompose(A)
```

## Mathematical Formulation

With a permutation `P` for partial pivoting:

$$ P A = L U $$

where `L` is unit-lower-triangular and `U` upper-triangular. Then `det(A) = ±∏ U_{ii}`.

> **Method:** Gaussian elimination with partial pivoting.

## Examples

```
{ [L, U] = LUDecompose(A) }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Matrix | Yes | Square matrix. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `L` | Matrix | Lower-triangular factor. |
| `U` | Matrix | Upper-triangular factor. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `SINGULAR_MATRIX` | a zero pivot remains | The matrix is singular; LU is not unique. |
