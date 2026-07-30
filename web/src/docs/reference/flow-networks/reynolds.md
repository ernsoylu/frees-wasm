---
name: reynolds
category: Flow Networks
summary: Reynolds number rho*V*D/mu
related: []
examples: []
tags: [reynolds, flow, networks]
---

# reynolds

Reynolds number rho*V*D/mu


## Syntax

```
reynolds(rho, V, D, mu)
```

## Description

Reynolds number rho*V*D/mu

## Mathematical Formulation

$$ Re = \frac{\rho V D}{\mu} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `rho` | Number | Yes | Density [kg/m³]. |
| `V` | Number | Yes | Velocity [m/s]. |
| `D` | Number | Yes | Diameter [m]. |
| `mu` | Number | Yes | Dynamic viscosity [Pa·s]. |
