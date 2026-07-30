---
name: mix_viscosity
category: Combustion
summary: Ideal-gas mixture viscosity [Pa-s] (Chapman-Enskog/Wilke)
related: []
examples: []
tags: [mix, viscosity, combustion]
---

# mix_viscosity

Ideal-gas mixture viscosity [Pa-s] (Chapman-Enskog/Wilke)


## Syntax

```
mix_viscosity(comp$, T)
```

## Description

Ideal-gas mixture viscosity [Pa-s] (Chapman-Enskog/Wilke)

## Mathematical Formulation

$$ \mu = \sum_i \frac{y_i \mu_i}{\sum_j y_j \phi_{ij}} \quad\text{(Wilke)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `comp$` | String | Yes | Mixture composition string, e.g. 'N2:0.79,O2:0.21'. |
| `T` | Number | Yes | Temperature [K]. |
