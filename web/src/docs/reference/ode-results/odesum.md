---
name: odesum
category: ODE Results
summary: Sum of an ODE column
related: []
examples: []
tags: [odesum, ode, results]
references: []
---

# odesum

Sum of an ODE column


## Syntax

```
ODESum('col')
```

## Description

Sum of an ODE column

## Mathematical Formulation

$$ \sum_{i=0}^{N} \text{col}(t_i) $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'col'` | Number | Yes | Name of a result-table column (string). |
