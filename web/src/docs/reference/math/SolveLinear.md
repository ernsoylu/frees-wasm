---
name: SolveLinear
category: Math
summary: Solve A·x = b (same as A \\ b)
related: []
examples: []
tags: [solvelinear, math]
---

# SolveLinear

Solve A·x = b (same as A \\ b)


## Syntax

```
SolveLinear(A, b)
```

## Description

Solve A·x = b (same as A \\ b)

## Mathematical Formulation

$$ A\,x = b \;\Rightarrow\; x = A^{-1}b \quad\text{(via } PA = LU\text{, forward/back substitution)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A` | Number | Yes | Matrix. |
| `b` | Number | Yes | Second operand. |
