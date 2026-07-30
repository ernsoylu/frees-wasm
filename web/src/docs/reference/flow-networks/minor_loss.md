---
name: minor_loss
category: Flow Networks
summary: Minor (fitting) pressure loss K*0.5*rho*V^2 [Pa]
related: []
examples: []
tags: [minor, loss, flow, networks]
---

# minor_loss

Minor (fitting) pressure loss K*0.5*rho*V^2 [Pa]


## Syntax

```
minor_loss(K, rho, V)
```

## Description

Minor (fitting) pressure loss K*0.5*rho*V^2 [Pa]

## Mathematical Formulation

$$ \Delta P = K\,\tfrac12\rho V^2 $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `K` | Number | Yes | Loss coefficient / gain. |
| `rho` | Number | Yes | Density [kg/m³]. |
| `V` | Number | Yes | Velocity [m/s]. |
