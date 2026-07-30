---
name: interpolate
category: Interpolation
summary: Linear interpolation of table t at x (same as t(x))
related: []
examples: []
tags: [interpolate, interpolation]
---

# interpolate

Linear interpolation of table t at x (same as t(x))


## Syntax

```
Interpolate('t', x)
```

## Description

Linear interpolation of table t at x (same as t(x))

## Mathematical Formulation

$$ y = y_i + (y_{i+1}-y_i)\frac{x - x_i}{x_{i+1} - x_i} \quad\text{(linear)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'t'` | Number | Yes | Name of a TABLE block (string). |
| `x` | Number | Yes | Vapor quality (0–1). |
