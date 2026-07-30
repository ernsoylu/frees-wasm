---
name: parallel
category: Control Systems
summary: Parallel connection of two transfer functions, G = G1 + G2.
related: [series, feedback]
examples: []
tags: [control, parallel, block diagram, transfer function, sum]
---

# parallel

Returns the **parallel connection** of two transfer functions —
`G(s) = G1(s) + G2(s)` — as a single `num/den` pair. It models two blocks fed the
same input whose outputs are summed.

## Syntax

```
CALL parallel(num1, den1, num2, den2 : num, den)
[num, den] = parallel(num1, den1, num2, den2)
```

## Mathematical Formulation

$$ G(s) = G_1(s) + G_2(s) = \frac{\text{num}_1\,\text{den}_2 + \text{num}_2\,\text{den}_1}{\text{den}_1\,\text{den}_2} $$

> **Method:** common-denominator polynomial addition.

## Examples

```
{ [num, den] = parallel(num1, den1, num2, den2) }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `num1`, `den1` | Vector | Yes | First transfer function `G1`. |
| `num2`, `den2` | Vector | Yes | Second transfer function `G2`. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `num` | Vector | Numerator of `G1 + G2`. |
| `den` | Vector | Denominator of `G1 + G2`. |
