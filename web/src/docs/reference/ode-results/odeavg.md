---
name: odeavg
category: ODE Results
summary: Time-mean of an ODE column
related: []
examples: []
tags: [odeavg, ode, results]
references: []
---

# odeavg

Time-mean of an ODE column


## Syntax

```
ODEAvg('col')
```

## Description

Time-mean of an ODE column

## Mathematical Formulation

$$ \frac{1}{N+1}\sum_{i=0}^{N} \text{col}(t_i) $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'col'` | Number | Yes | Name of a result-table column (string). |
