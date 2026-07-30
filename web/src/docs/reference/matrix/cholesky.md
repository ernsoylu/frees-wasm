---
name: cholesky
category: Matrix
summary: Cholesky decomposition
related: []
examples: []
tags: [cholesky, matrix]
---

# cholesky

Cholesky decomposition


## Syntax

```
cholesky(A : L)
```

## Description

Cholesky decomposition

## Mathematical Formulation

$$ A = L\,L^\top \quad\text{(} A \text{ symmetric positive-definite)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Number | Yes | Square input matrix. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `L` | Number/Array | Length [m]. |
