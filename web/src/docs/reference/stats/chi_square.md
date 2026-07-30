---
name: chi_square
category: Stats
summary: Chi-square CDF with df degrees of freedom
related: []
examples: []
tags: [chi, square, stats]
---

# chi_square

Chi-square CDF with df degrees of freedom


## Syntax

```
chi_square(x, df)
```

## Description

Chi-square CDF with df degrees of freedom

## Mathematical Formulation

$$ F(x; k) = \frac{\gamma(k/2,\ x/2)}{\Gamma(k/2)} \quad\text{(chi-square CDF, } k \text{ d.o.f.)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Vapor quality (0–1). |
| `df` | Number | Yes | Degrees of freedom. |
