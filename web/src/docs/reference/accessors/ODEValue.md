---
name: ODEValue
category: ODE Results
summary: Value of an ODE (DYNAMIC) column interpolated at a given time.
related: [FinalValue, MaxValue, TimeAt]
examples: [damped-oscillator-ode]
tags: [ode, dynamic, accessor, interpolation, time, trajectory]
references: []
---

# ODEValue

Returns the value of a named `DYNAMIC` (ODE) column **interpolated at an arbitrary
time** `t` within the integration window. Use it to sample a transient at a
specific instant that need not coincide with an integration step.

## Syntax

```
v = ODEValue('col', t)
```

## Description

Because the adaptive integrator places samples unevenly, `ODEValue` linearly
interpolates the column between the bracketing samples to return the value at the
requested time.

## Mathematical Formulation

For `t` bracketed by samples $t_i \le t \le t_{i+1}$,

$$ \text{ODEValue}('col', t) = \text{col}(t_i) + \big(\text{col}(t_{i+1}) - \text{col}(t_i)\big)\frac{t - t_i}{t_{i+1} - t_i} $$

> **Method:** linear interpolation between the two bracketing integration samples.

## Examples

### Example 1 — Sample a transient at a chosen instant

[Run: damped-oscillator-ode]

**Expected:** the column value at the requested time, interpolated from the ODE
trajectory.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'col'` | String | Yes | Name of a state or auxiliary column. |
| `t` | Number | Yes | Time at which to sample (within the integration window). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `v` | Number | The interpolated column value at time `t`. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `TIME_OUT_OF_RANGE` | `t` outside the integration window | Sample within `[t0, tf]` of the `DYNAMIC` block. |
| `UNKNOWN_COLUMN` | `'col'` not a column | Use a state name or declared auxiliary. |
