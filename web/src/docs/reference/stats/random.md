---
name: random
category: Stats
summary: Uniform random number in [a, b]
related: []
examples: []
tags: [random, stats]
references: []
---

# random

Uniform random number in [a, b]


## Syntax

```
random(a, b)
```

## Description

Uniform random number in [a, b]

## Mathematical Formulation

$$ X \sim \mathcal{U}(a, b), \qquad X = a + (b-a)\,U,\ \ U\in[0,1) $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `a` | Number | Yes | First operand. |
| `b` | Number | Yes | Second operand. |
