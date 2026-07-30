---
name: gaussintegral
category: Calculus
summary: Definite integral by Gauss-Legendre quadrature
related: []
examples: []
tags: [gaussintegral, calculus]
---

# gaussintegral

Definite integral by Gauss-Legendre quadrature


## Syntax

```
GaussIntegral(expr, var, lower, upper)
```

## Description

Definite integral by Gauss-Legendre quadrature

## Mathematical Formulation

$$ \int_a^b f(x)\,dx \approx \frac{b-a}{2}\sum_{i=1}^{n} w_i\,f\!\left(\tfrac{b-a}{2}\xi_i + \tfrac{a+b}{2}\right) \quad\text{(Gauss–Legendre)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `expr` | Number | Yes | Expression to evaluate. |
| `var` | Number | Yes | Integration variable. |
| `lower` | Number | Yes | Lower limit. |
| `upper` | Number | Yes | Upper limit. |
