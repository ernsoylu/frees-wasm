---
name: percentile
category: Stats
summary: p-th percentile, p in [0,100]
related: []
examples: []
tags: [percentile, stats]
---

# percentile

p-th percentile, p in [0,100]


## Syntax

```
percentile(p, x1, x2, ...)
```

## Description

p-th percentile, p in [0,100]

## Mathematical Formulation

$$ P_p = \text{value below which } p\% \text{ of the data fall (linear interpolation)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `p` | Number | Yes | Probability (0–1) / percentile rank. |
| `x1` | Number | Yes | First value. |
| `x2` | Number | Yes | Second value. |
| `...` | Number | Yes | Additional values (variadic). |
