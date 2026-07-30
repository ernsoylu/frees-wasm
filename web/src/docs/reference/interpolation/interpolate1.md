---
name: interpolate1
category: Interpolation
summary: Cubic-spline interpolation of table t at x
related: []
examples: []
tags: [interpolate1, interpolation]
---

# interpolate1

Cubic-spline interpolation of table t at x


## Syntax

```
Interpolate1('t', x)
```

## Description

Cubic-spline interpolation of table t at x

## Mathematical Formulation

$$ \text{piecewise cubic spline through the table knots (} C^2 \text{ continuous)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'t'` | Number | Yes | Name of a TABLE block (string). |
| `x` | Number | Yes | Vapor quality (0–1). |
