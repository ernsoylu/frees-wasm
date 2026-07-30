---
name: Eigen
category: Matrix
summary: Eigenvalues and eigenvectors of a square matrix.
related: [Eigenvalues, balreal]
examples: []
tags: [matrix, eigenvalues, eigenvectors, spectral, linear algebra]
---

# Eigen

Returns the **eigenvalues** `lambda` and **eigenvectors** `V` of a square matrix
`A` — the full eigendecomposition `A V = V Λ`. The eigenvectors give the modal
directions; the eigenvalues their rates/frequencies.

## Syntax

```
CALL Eigen(A : lambda, V)
[lambda, V] = Eigen(A)
```

## Mathematical Formulation

$$ A\,v_i = \lambda_i\,v_i, \qquad A = V\,\Lambda\,V^{-1} $$

where `Λ = diag(λ_i)` and the columns of `V` are the eigenvectors.

> **Method:** QR algorithm with eigenvector back-substitution.

Eigen supports **real spectra only** (symmetric matrices always qualify) and
stops with an error on complex eigenvalues; for a complex spectrum use
`CALL Eigenvalues(A : re, im)`, which returns real/imaginary part vectors.

## Examples

```
{ [lambda, V] = Eigen(A) }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Matrix | Yes | Square matrix. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `lambda` | Vector | Eigenvalues. |
| `V` | Matrix | Eigenvectors (columns). |
