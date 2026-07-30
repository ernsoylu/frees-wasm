---
name: rayleigh_p_pstar
category: Compressible Flow
summary: Rayleigh static-pressure ratio
related: []
examples: []
tags: [rayleigh, pstar, compressible, flow]
---

# rayleigh_p_pstar

Rayleigh static-pressure ratio


## Syntax

```
rayleigh_P_Pstar(M, k)
```

## Description

Rayleigh static-pressure ratio

## Mathematical Formulation

$$ \frac{P}{P^*} = \frac{k+1}{1 + kM^2} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `M` | Number | Yes | Mach number. |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |
