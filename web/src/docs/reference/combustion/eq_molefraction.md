---
name: eq_molefraction
category: Combustion
summary: Equilibrium product mole fraction (dissociation)
related: []
examples: []
tags: [eq, molefraction, combustion]
---

# eq_molefraction

Equilibrium product mole fraction (dissociation)


## Syntax

```
eq_molefraction(fuel$, phi, T, P, species$)
```

## Description

Equilibrium product mole fraction (dissociation)

## Mathematical Formulation

$$ \text{species mole fraction from chemical equilibrium } \big(\min G \text{ at } T, P\big) $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `fuel$` | String | Yes | Fuel name/formula (e.g. 'CH4'). |
| `phi` | Number | Yes | Equivalence ratio (1 = stoichiometric). |
| `T` | Number | Yes | Temperature [K]. |
| `P` | Number | Yes | Pressure [Pa]. |
| `species$` | String | Yes | Product species name (e.g. CO, NO). |
