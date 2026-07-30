---
name: t2_t1_shock
category: Compressible Flow
summary: Normal-shock static temperature ratio
related: []
examples: []
tags: [t2, t1, shock, compressible, flow]
---

# t2_t1_shock

Normal-shock static temperature ratio


## Syntax

```
T2_T1_shock(M1, k)
```

## Description

Normal-shock static temperature ratio

## Mathematical Formulation

$$ \frac{T_2}{T_1} = \frac{\big[1 + \tfrac{k-1}{2}M_1^2\big]\big[\tfrac{2k}{k-1}M_1^2 - 1\big]}{M_1^2\,(k+1)^2/[2(k-1)]} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `M1` | Number | Yes | Upstream Mach number (≥ 1). |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |
