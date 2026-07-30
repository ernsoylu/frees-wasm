---
name: rayleigh_t_tstar
category: Compressible Flow
summary: Rayleigh static-temperature ratio
related: []
examples: []
tags: [rayleigh, tstar, compressible, flow]
---

# rayleigh_t_tstar

Rayleigh static-temperature ratio


## Syntax

```
rayleigh_T_Tstar(M, k)
```

## Description

Rayleigh static-temperature ratio

## Mathematical Formulation

$$ \frac{T}{T^*} = \left(\frac{(k+1)M}{1 + kM^2}\right)^2 $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `M` | Number | Yes | Mach number. |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |
