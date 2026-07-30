---
name: Eigenvalues
category: Matrix
summary: Eigenvalues of a square matrix.
related: [Eigen, Determinant, cond]
examples: []
tags: [matrix, eigenvalues, spectrum, linear algebra]
---

# Eigenvalues

Returns the **eigenvalues** `lambda` of a square matrix `A` — the scalars `λ` for
which `A v = λ v` has a nonzero solution. They set system stability (continuous:
left half-plane; discrete: inside the unit circle) and modal frequencies.

## Syntax

```
CALL Eigenvalues(A : lambda)
CALL Eigenvalues(A : re, im)
lambda = Eigenvalues(A)
```

## Mathematical Formulation

The eigenvalues are the roots of the characteristic polynomial:

$$ \det(A - \lambda I) = 0 $$

> **Method:** QR algorithm on the (balanced) matrix.

The single-output form supports **real spectra only** (symmetric matrices always
qualify) and stops with an error on complex eigenvalues. The two-output form
carries a complex spectrum as real/imaginary part vectors; eigenvalues are
sorted ascending by real part, then imaginary part.

## Examples

```
{ lambda = Eigenvalues(A) }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Matrix | Yes | Square matrix. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `lambda` | Vector | Eigenvalues, ascending (single-output form; real spectra only). |
| `re`, `im` | Vector | Real and imaginary parts of the spectrum (two-output form). |
