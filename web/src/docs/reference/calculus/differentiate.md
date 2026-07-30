---
name: differentiate
category: Calculus
summary: Numerical dy/dx at xv from a TABLE
related: []
examples: []
tags: [differentiate, calculus]
---

# differentiate

Numerical dy/dx at xv from a TABLE


## Syntax

```
Differentiate('t', y, x, xv)
```

## Description

Numerical dy/dx at xv from a TABLE

## Mathematical Formulation

$$ \left.\frac{dy}{dx}\right|_{x_v} \approx \frac{y_{i+1}-y_{i-1}}{x_{i+1}-x_{i-1}} \quad\text{(central difference on the table)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'t'` | Number | Yes | Name of a TABLE block (string). |
| `y` | Number | Yes | Value / second coordinate. |
| `x` | Number | Yes | Vapor quality (0–1). |
| `xv` | Number | Yes | Point at which to evaluate. |
