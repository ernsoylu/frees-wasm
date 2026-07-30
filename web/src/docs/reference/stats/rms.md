---
name: rms
category: Stats
summary: Root mean square
related: []
examples: []
tags: [rms, stats]
references: []
---

# rms

Root mean square


## Syntax

```
rms(x1, x2, ...)
```

## Description

Root mean square

## Mathematical Formulation

$$ x_{\text{rms}} = \sqrt{\frac{1}{n}\sum_{i=1}^{n} x_i^2} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x1` | Number | Yes | First value. |
| `x2` | Number | Yes | Second value. |
| `...` | Number | Yes | Additional values (variadic). |
