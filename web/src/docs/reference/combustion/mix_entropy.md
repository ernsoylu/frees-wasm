---
name: mix_entropy
category: Combustion
summary: Ideal-gas mixture entropy [J/kg-K]
related: []
examples: []
tags: [mix, entropy, combustion]
---

# mix_entropy

Ideal-gas mixture entropy [J/kg-K]


## Syntax

```
mix_entropy(comp$, T, P)
```

## Description

Ideal-gas mixture entropy [J/kg-K]

## Mathematical Formulation

$$ s = \sum_i Y_i\big[s_i(T) - R_i\ln(y_i P/P_0)\big] $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `comp$` | String | Yes | Mixture composition string, e.g. 'N2:0.79,O2:0.21'. |
| `T` | Number | Yes | Temperature [K]. |
| `P` | Number | Yes | Pressure [Pa]. |
