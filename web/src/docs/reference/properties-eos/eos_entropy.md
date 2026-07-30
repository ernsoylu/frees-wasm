---
name: eos_entropy
category: Properties (EOS)
summary: Specific entropy [J/kg-K] (SRK/PR)
related: []
examples: []
tags: [eos, entropy, properties]
---

# eos_entropy

Specific entropy [J/kg-K] (SRK/PR)


## Syntax

```
eos_entropy(fluid$, model$, T, P, phase$)
```

## Description

Specific entropy [J/kg-K] (SRK/PR)

## Mathematical Formulation

$$ s(T,P) = s^{\text{ig}}(T,P) + (s - s^{\text{ig}})_{T,P} \quad\text{(ideal-gas + EOS departure)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `fluid$` | String | Yes | Fluid name (e.g. Water, R134a, Air). |
| `model$` | String | Yes | Selector — One of `SRK`, `PR`. |
| `T` | Number | Yes | Temperature [K]. |
| `P` | Number | Yes | Pressure [Pa]. |
| `phase$` | String | Yes | Selector — One of `vapor`, `liquid`. |
