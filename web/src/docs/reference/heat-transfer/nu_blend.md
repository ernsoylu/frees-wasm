---
name: nu_blend
category: Heat Transfer
summary: Cubic free+forced blend (Nu1^3+Nu2^3)^(1/3). USE: combine natural + forced Nu on any side
related: []
examples: []
tags: [nu, blend, heat, transfer]
---

# nu_blend

Cubic free+forced blend (Nu1^3+Nu2^3)^(1/3). USE: combine natural + forced Nu on any side


## Syntax

```
nu_blend(Nu1, Nu2)
```

## Description

Cubic free+forced blend (Nu1^3+Nu2^3)^(1/3). USE: combine natural + forced Nu on any side

## Mathematical Formulation

$$ Nu = \big(Nu_1^3 + Nu_2^3\big)^{1/3} \quad\text{(free+forced cubic blend)} $$

## Applicability

- **Where it applies:** Any surface with combined natural + forced convection.
- **Valid when:** Mixed convection where neither mechanism dominates.
- **How it's used:** Combines two Nusselt numbers as `(Nu₁³ + Nu₂³)^{1/3}`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Nu1` | Number | Yes | First Nusselt number to blend. |
| `Nu2` | Number | Yes | Second Nusselt number to blend. |
