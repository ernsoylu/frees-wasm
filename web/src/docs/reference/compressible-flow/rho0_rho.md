---
name: rho0_rho
category: Compressible Flow
summary: Isentropic stagnation/static density ratio
related: []
examples: []
tags: [rho0, rho, compressible, flow]
---

# rho0_rho

Isentropic stagnation/static density ratio


## Syntax

```
rho0_rho(M, k)
```

## Description

Isentropic stagnation/static density ratio

## Mathematical Formulation

$$ \frac{\rho_0}{\rho} = \left(1 + \tfrac{k-1}{2}M^2\right)^{1/(k-1)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `M` | Number | Yes | Mach number. |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |
