---
name: friction_factor
category: Flow Networks
summary: Darcy friction factor (Colebrook-Moody, laminar+turbulent)
related: []
examples: []
tags: [friction, factor, flow, networks]
---

# friction_factor

Darcy friction factor (Colebrook-Moody, laminar+turbulent)


## Syntax

```
friction_factor(Re, rel_rough)
```

## Description

Darcy friction factor (Colebrook-Moody, laminar+turbulent)

## Mathematical Formulation

$$ \frac{1}{\sqrt{f}} = -2\log_{10}\!\left(\frac{\varepsilon/D}{3.7} + \frac{2.51}{Re\sqrt{f}}\right) \quad\text{(Colebrook; } f = 64/Re \text{ laminar)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Re` | Number | Yes | Reynolds number. |
| `rel_rough` | Number | Yes | Relative wall roughness ε/D. |
