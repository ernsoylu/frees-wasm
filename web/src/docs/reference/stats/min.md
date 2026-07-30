---
name: min
category: Stats
summary: Smallest of the arguments.
related: [max, average, percentile]
examples: [hx-effectiveness-ntu, ev-thermal-management]
tags: [stats, minimum, comparison, elementary]
references: []
---

# min

Returns the **smallest** of its arguments. Commonly used to pick the limiting of
two quantities — e.g. `C_min = min(C_h, C_c)` in heat-exchanger analysis.

## Syntax

```
y = min(a, b, ...)
```

## Description

Accepts two or more numeric arguments and returns the least. Units must be
compatible across the arguments.

## Mathematical Formulation

$$ y = \min(a_1, a_2, \dots, a_n) $$

## Examples

### Example 1 — Minimum capacity rate of a heat exchanger

[Run: hx-effectiveness-ntu]

**Expected:** `C_min = min(C_h, C_c)` selects the smaller heat-capacity rate.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `a, b, …` | Number | Yes | Two or more values with compatible units. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | The smallest argument. |
