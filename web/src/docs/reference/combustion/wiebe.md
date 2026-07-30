---
name: wiebe
category: Combustion
summary: Wiebe burned mass fraction
related: []
examples: []
tags: [wiebe, combustion]
---

# wiebe

Wiebe burned mass fraction


## Syntax

```
wiebe(theta, theta0, dtheta, a, m)
```

## Description

Wiebe burned mass fraction

## Mathematical Formulation

$$ x_b(\theta) = 1 - \exp\!\left[-a\left(\frac{\theta-\theta_0}{\Delta\theta}\right)^{m+1}\right] $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `theta` | Number | Yes | Flow-deflection angle [rad]. |
| `theta0` | Number | Yes | Start of combustion [deg]. |
| `dtheta` | Number | Yes | Combustion duration [deg]. |
| `a` | Number | Yes | First operand. |
| `m` | Number | Yes | Shape / form parameter. |
