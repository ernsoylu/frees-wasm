---
name: qr
category: Matrix
summary: QR decomposition
related: []
examples: []
tags: [qr, matrix]
---

# qr

QR decomposition


## Syntax

```
qr(A : Q, R)
```

## Description

QR decomposition

## Mathematical Formulation

$$ A = Q\,R, \qquad Q^\top Q = I,\ R\ \text{upper triangular} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Number | Yes | Square input matrix. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `Q` | Number/Array | Computed `Q`. |
| `R` | Number/Array | Computed `R`. |
