---
name: mix_enthalpy
category: Combustion
summary: Ideal-gas mixture enthalpy [J/kg]
related: []
examples: []
tags: [mix, enthalpy, combustion]
---

# mix_enthalpy

Ideal-gas mixture enthalpy [J/kg]


## Syntax

```
mix_enthalpy(comp$, T)
```

## Description

Ideal-gas mixture enthalpy [J/kg]

## Mathematical Formulation

$$ h = \sum_i Y_i\,h_i(T) \quad\text{(NASA-7 polynomials)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `comp$` | String | Yes | Mixture composition string, e.g. 'N2:0.79,O2:0.21'. |
| `T` | Number | Yes | Temperature [K]. |
