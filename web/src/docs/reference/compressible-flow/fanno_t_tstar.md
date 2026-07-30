---
name: fanno_t_tstar
category: Compressible Flow
summary: Fanno static-temperature ratio
related: []
examples: []
tags: [fanno, tstar, compressible, flow]
---

# fanno_t_tstar

Fanno static-temperature ratio


## Syntax

```
fanno_T_Tstar(M, k)
```

## Description

Fanno static-temperature ratio

## Mathematical Formulation

$$ \frac{T}{T^*} = \frac{k+1}{2 + (k-1)M^2} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `M` | Number | Yes | Mach number. |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |
