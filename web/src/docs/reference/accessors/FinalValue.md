---
name: FinalValue
category: ODE Results
summary: Last value of a column in the integrated ODE (DYNAMIC) table.
related: [MaxValue, MinValue, ODEValue, TimeAt]
examples: [newton-cooling-transient]
tags: [ode, dynamic, accessor, final value, transient, trajectory]
references: []
---

# FinalValue

Returns the **last value** of a named column of the integrated `DYNAMIC` (ODE)
result table — the value at the end of the integration window. Use it to feed a
transient result back into the analytic solve (e.g. close a sizing loop on a final
temperature).

## Syntax

```
v = FinalValue('col')
```

## Description

After a `DYNAMIC` block integrates, every state and auxiliary becomes a column of
the result table. `FinalValue` reads the last row of the requested column, so a
transient endpoint can drive an algebraic equation.

## Mathematical Formulation

For a column sampled at times $t_0 < t_1 < \dots < t_N$,

$$ \text{FinalValue}('col') = \text{col}(t_N) $$

> **Method:** read the last sample of the column (no interpolation).

## Examples

### Example 1 — Final temperature of a cooling transient

[Run: newton-cooling-transient]

**Expected:** the temperature at the end of the integration window, used as a
scalar in the analytic part of the document.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'col'` | String | Yes | Name of a state or auxiliary column in the `DYNAMIC` table. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `v` | Number | The column value at the final integration time. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `UNKNOWN_COLUMN` | `'col'` is not a column of the ODE table | Use a state name or a declared auxiliary from the `DYNAMIC` block. |
| `NO_DYNAMIC_RESULT` | No `DYNAMIC` block has been integrated | Define and solve a `DYNAMIC` block first. |
