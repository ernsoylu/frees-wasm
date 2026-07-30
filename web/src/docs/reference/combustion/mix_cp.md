---
name: mix_cp
category: Combustion
summary: Ideal-gas mixture cp [J/kg-K]
related: []
examples: []
tags: [mix, cp, combustion]
---

# mix_cp

Ideal-gas mixture cp [J/kg-K]


## Syntax

```
mix_cp(comp$, T)
```

## Description

Ideal-gas mixture cp [J/kg-K]

## Mathematical Formulation

$$ c_p = \sum_i Y_i\,c_{p,i}(T) \quad\text{(mass-weighted, NASA-7)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `comp$` | String | Yes | Mixture composition string, e.g. 'N2:0.79,O2:0.21'. |
| `T` | Number | Yes | Temperature [K]. |
