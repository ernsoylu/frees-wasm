---
name: mix_conductivity
category: Combustion
summary: Ideal-gas mixture conductivity [W/m-K]
related: []
examples: []
tags: [mix, conductivity, combustion]
---

# mix_conductivity

Ideal-gas mixture conductivity [W/m-K]


## Syntax

```
mix_conductivity(comp$, T)
```

## Description

Ideal-gas mixture conductivity [W/m-K]

## Mathematical Formulation

$$ \lambda = \sum_i \frac{y_i \lambda_i}{\sum_j y_j \phi_{ij}} \quad\text{(Wassiljewa/Wilke)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `comp$` | String | Yes | Mixture composition string, e.g. 'N2:0.79,O2:0.21'. |
| `T` | Number | Yes | Temperature [K]. |
