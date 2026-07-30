---
name: gemm
category: Math
summary: BLAS L3: αAB + βC
related: []
examples: []
tags: [gemm, math]
references: []
---

# gemm

BLAS L3: αAB + βC


## Syntax

```
gemm(α, A, B, β, C)
```

## Description

BLAS L3: αAB + βC

## Mathematical Formulation

$$ C \leftarrow \alpha A B + \beta C \quad\text{(BLAS level 3)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `α` | Number | Yes | Scalar coefficient α. |
| `A` | Number | Yes | Matrix. |
| `B` | Number | Yes | Matrix operand. |
| `β` | Number | Yes | Scalar coefficient β. |
| `C` | Number | Yes | Empirical constant. |
