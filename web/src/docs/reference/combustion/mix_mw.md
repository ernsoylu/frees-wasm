---
name: mix_mw
category: Combustion
summary: Ideal-gas mixture molar mass [kg/mol], comp 'N2:0.79,O2:0.21'
related: []
examples: []
tags: [mix, mw, combustion]
---

# mix_mw

Ideal-gas mixture molar mass [kg/mol], comp 'N2:0.79,O2:0.21'


## Syntax

```
mix_mw(comp$)
```

## Description

Ideal-gas mixture molar mass [kg/mol], comp 'N2:0.79,O2:0.21'

## Mathematical Formulation

$$ \overline{M} = \sum_i y_i M_i $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `comp$` | String | Yes | Mixture composition string, e.g. 'N2:0.79,O2:0.21'. |
