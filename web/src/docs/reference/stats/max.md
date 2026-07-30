---
name: max
category: Stats
summary: Largest of the arguments.
related: [min, average, percentile]
examples: [hx-effectiveness-ntu]
tags: [stats, maximum, comparison, elementary]
references: []
---

# max

Returns the **largest** of its arguments — e.g. `C_max = max(C_h, C_c)` in
heat-exchanger analysis.

## Syntax

```
y = max(a, b, ...)
```

## Description

Accepts two or more numeric arguments and returns the greatest. Units must be
compatible across the arguments.

## Mathematical Formulation

$$ y = \max(a_1, a_2, \dots, a_n) $$

## Examples

### Example 1 — Maximum capacity rate of a heat exchanger

[Run: hx-effectiveness-ntu]

**Expected:** `C_max = max(C_h, C_c)` selects the larger heat-capacity rate.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `a, b, …` | Number | Yes | Two or more values with compatible units. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | The largest argument. |
