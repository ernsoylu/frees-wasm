---
name: a_astar
category: Compressible Flow
summary: Isentropic area ratio A/A*
related: []
examples: []
tags: [astar, compressible, flow]
---

# a_astar

Isentropic area ratio A/A*


## Syntax

```
A_Astar(M, k)
```

## Description

Isentropic area ratio A/A*

## Mathematical Formulation

$$ \frac{A}{A^*} = \frac{1}{M}\left[\frac{2}{k+1}\left(1 + \tfrac{k-1}{2}M^2\right)\right]^{(k+1)/[2(k-1)]} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `M` | Number | Yes | Mach number. |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |
