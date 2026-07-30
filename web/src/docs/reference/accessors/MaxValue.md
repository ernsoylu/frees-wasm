---
name: MaxValue
category: ODE Results
summary: Peak value of a column in the integrated ODE (DYNAMIC) table.
related: [MinValue, FinalValue, TimeAt, ODEValue]
examples: [newton-cooling-transient]
tags: [ode, dynamic, accessor, maximum, peak, transient, trajectory]
references: []
---

# MaxValue

Returns the **peak value** of a named column of the integrated `DYNAMIC` (ODE)
result table over the whole integration window. Use it to size against a transient
maximum — e.g. peak overshoot, peak temperature, or peak altitude.

## Syntax

```
v = MaxValue('col')
```

## Description

`MaxValue` scans the requested column across all integration samples and returns
its largest value, so a transient peak can drive an algebraic equation (a common
sizing pattern: `MaxValue('h') = h_target`).

## Mathematical Formulation

$$ \text{MaxValue}('col') = \max_{0 \le i \le N} \text{col}(t_i) $$

> **Method:** maximum over the column's samples.

## Examples

### Example 1 — Peak of a transient

[Run: newton-cooling-transient]

**Expected:** the largest value the column reaches during the integration.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'col'` | String | Yes | Name of a state or auxiliary column in the `DYNAMIC` table. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `v` | Number | The maximum of the column over the run. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `UNKNOWN_COLUMN` | `'col'` is not a column of the ODE table | Use a state name or a declared auxiliary. |
| `NO_DYNAMIC_RESULT` | No `DYNAMIC` block integrated | Define and solve a `DYNAMIC` block first. |
