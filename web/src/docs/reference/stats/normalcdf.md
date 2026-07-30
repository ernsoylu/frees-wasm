---
name: normalcdf
category: Stats
summary: Normal cumulative distribution at x
related: []
examples: []
tags: [normalcdf, stats]
---

# normalcdf

Normal cumulative distribution at x


## Syntax

```
normalcdf(x, mu, sigma)
```

## Description

Normal cumulative distribution at x

## Mathematical Formulation

$$ \Phi(x;\mu,\sigma) = \tfrac12\left[1 + \operatorname{erf}\!\frac{x-\mu}{\sigma\sqrt2}\right] $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Vapor quality (0–1). |
| `mu` | Number | Yes | Dynamic viscosity [Pa·s]. |
| `sigma` | Number | Yes | Surface tension [N/m]. |
