---
name: tablestddev
category: Tables
summary: Standard deviation of a parametric-table column
related: []
examples: []
tags: [tablestddev, tables]
references: []
---

# tablestddev

Standard deviation of a parametric-table column


## Syntax

```
TableStdDev('col')
```

## Description

Standard deviation of a parametric-table column

## Mathematical Formulation

$$ s = \sqrt{\tfrac{1}{n-1}\sum_r (c_r - \bar c)^2} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'col'` | Number | Yes | Name of a result-table column (string). |
