---
name: eos_pressure
category: Properties (EOS)
summary: Pressure [Pa] from (T, specific volume)
related: []
examples: []
tags: [eos, pressure, properties]
---

# eos_pressure

Pressure [Pa] from (T, specific volume)


## Syntax

```
eos_pressure(fluid$, model$, T, v)
```

## Description

Pressure [Pa] from (T, specific volume)

## Mathematical Formulation

$$ P = \frac{RT}{v-b} - \frac{a\,\alpha(T)}{v(v+b) + b(v-b)} \quad\text{(PR; from } T, v) $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `fluid$` | String | Yes | Fluid name (e.g. Water, R134a, Air). |
| `model$` | String | Yes | Selector — One of `SRK`, `PR`. |
| `T` | Number | Yes | Temperature [K]. |
| `v` | Number | Yes | Specific volume [m³/kg]. |
