---
name: diag
category: Matrix
summary: Diagonal matrix built from a vector.
related: [eye, zeros, Transpose]
examples: [estimator-gramian-balreal]
tags: [matrix, diagonal, construction, linear algebra]
references: []
---

# diag

Builds a square **diagonal matrix** whose diagonal is the supplied vector and whose
off-diagonal entries are zero. Common for assembling weighting matrices (e.g. the
`Q`/`R` of an LQR/LQE design).

## Syntax

```
M = diag(v)
```

## Description

For a length-`n` vector `v`, returns the `n×n` matrix `M` with `M[i,i] = v[i]`.

## Mathematical Formulation

$$ M_{ij} = \begin{cases} v_i & i = j \\ 0 & i \neq j \end{cases} $$

## Examples

### Example 1 — Weighting matrix for an estimator design

[Run: estimator-gramian-balreal]

**Expected:** a diagonal matrix used as a noise/weighting matrix in the design.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `v` | Vector | Yes | The diagonal entries. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `M` | Matrix | `n×n` diagonal matrix. |
