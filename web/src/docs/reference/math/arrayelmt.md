---
name: arrayelmt
category: Math
summary: Select the i-th element of an array range
related: []
examples: []
tags: [arrayelmt, math]
references: []
---

# arrayelmt

Select the i-th element of an array range


## Syntax

```
ArrayElmt(arr[1:n], i)
```

## Description

Select the i-th element of an array range

## Mathematical Formulation

$$ \operatorname{ArrayElmt}(\{a_1,\dots,a_n\}, i) = a_i $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `arr[1:n]` | Array | Yes | Array range to index into, e.g. `data[1:n]`. |
| `i` | Number | Yes | 1-based index of the element to return. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `n]` | Number/Array | Computed `n]`. |
| `i` | Number/Array | Computed `i`. |
