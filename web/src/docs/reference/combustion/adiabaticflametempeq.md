---
name: adiabaticflametempeq
category: Combustion
summary: Adiabatic flame temperature with dissociation [K]
related: []
examples: []
tags: [adiabaticflametempeq, combustion]
---

# adiabaticflametempeq

Adiabatic flame temperature with dissociation [K]


## Syntax

```
AdiabaticFlameTempEq(fuel$, phi, T_react, P)
```

## Description

Adiabatic flame temperature with dissociation [K]

## Mathematical Formulation

$$ H_{\text{react}}(T_r) = H_{\text{prod}}(T_{ad}) \quad\text{with equilibrium dissociation at } (T_{ad}, P) $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `fuel$` | String | Yes | Fuel name/formula (e.g. 'CH4'). |
| `phi` | Number | Yes | Equivalence ratio (1 = stoichiometric). |
| `T_react` | Number | Yes | Reactant inlet temperature [K]. |
| `P` | Number | Yes | Pressure [Pa]. |
