---
name: odestddev
category: ODE Results
summary: Standard deviation of an ODE column
related: []
examples: []
tags: [odestddev, ode, results]
references: []
---

# odestddev

Standard deviation of an ODE column


## Syntax

```
ODEStdDev('col')
```

## Description

Standard deviation of an ODE column

## Mathematical Formulation

$$ s = \sqrt{\tfrac{1}{N}\sum_i (\text{col}(t_i) - \overline{\text{col}})^2} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'col'` | Number | Yes | Name of a result-table column (string). |
