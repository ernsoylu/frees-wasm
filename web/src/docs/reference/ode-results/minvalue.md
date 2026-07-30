---
name: minvalue
category: ODE Results
summary: Minimum value of an ODE column
related: []
examples: []
tags: [minvalue, ode, results]
references: []
---

# minvalue

Minimum value of an ODE column


## Syntax

```
MinValue('col')
```

## Description

Minimum value of an ODE column

## Mathematical Formulation

$$ \min_{0 \le i \le N} \text{col}(t_i) $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'col'` | Number | Yes | Name of a result-table column (string). |
