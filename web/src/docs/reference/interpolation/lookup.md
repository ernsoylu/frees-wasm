---
name: lookup
category: Interpolation
summary: Cell value by 1-based row/col indices
related: []
examples: []
tags: [lookup, interpolation]
references: []
---

# lookup

Cell value by 1-based row/col indices


## Syntax

```
Lookup('t', row, col)
```

## Description

Cell value by 1-based row/col indices

## Mathematical Formulation

$$ \operatorname{Lookup}(t, r, c) = t_{r,c} \quad\text{(1-based cell)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'t'` | Number | Yes | Name of a TABLE block (string). |
| `row` | Number | Yes | Row index (1-based). |
| `col` | Number | Yes | Name of a result-table column. |
