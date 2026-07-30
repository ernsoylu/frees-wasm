---
name: rayleigh_t0_t0star
category: Compressible Flow
summary: Rayleigh stagnation-temperature ratio
related: []
examples: []
tags: [rayleigh, t0, t0star, compressible, flow]
---

# rayleigh_t0_t0star

Rayleigh stagnation-temperature ratio


## Syntax

```
rayleigh_T0_T0star(M, k)
```

## Description

Rayleigh stagnation-temperature ratio

## Mathematical Formulation

$$ \frac{T_0}{T_0^*} = \frac{(k+1)M^2\,[2 + (k-1)M^2]}{(1 + kM^2)^2} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `M` | Number | Yes | Mach number. |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |
