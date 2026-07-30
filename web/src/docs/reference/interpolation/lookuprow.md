---
name: lookuprow
category: Interpolation
summary: Row index where column col crosses val
related: []
examples: []
tags: [lookuprow, interpolation]
references: []
---

# lookuprow

Row index where column col crosses val


## Syntax

```
LookupRow('t', col, val)
```

## Description

Row index where column col crosses val

## Mathematical Formulation

$$ \text{row } r \text{ where column } c \text{ crosses } val \text{ (interpolated)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'t'` | Number | Yes | Name of a TABLE block (string). |
| `col` | Number | Yes | Name of a result-table column. |
| `val` | Number | Yes | Target value to cross. |
