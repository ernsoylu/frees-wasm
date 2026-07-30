---
name: IntegralValue
category: Tables
summary: Trapezoidal integral of one column versus another (ODE or table data).
related: [TableAvg, ODEValue, integral]
examples: [driving-cycle-energy]
tags: [accessor, integral, trapezoidal, table, ode, area]
---

# IntegralValue

Returns the **trapezoidal integral** of one column with respect to another — the
area under `y` plotted against `x` — over the sampled data of a `DYNAMIC` or table
result. Use it to accumulate a transient quantity, e.g. energy from power over a
drive cycle.

## Syntax

```
A = IntegralValue('y', 'x')
```

## Description

`IntegralValue` integrates the `y` column against the `x` column using the
composite trapezoidal rule over their shared samples.

## Mathematical Formulation

$$ A = \int y\,dx \approx \sum_{i=0}^{N-1} \frac{y_i + y_{i+1}}{2}\,(x_{i+1} - x_i) \qquad \text{(trapezoidal rule)} $$

> **Method:** composite trapezoidal quadrature over the column samples.

## Examples

### Example 1 — Energy from power over a drive cycle

[Run: driving-cycle-energy]

**Expected:** the integral of power versus time — the cycle energy [J].

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'y'` | String | Yes | Integrand column. |
| `'x'` | String | Yes | Integration-variable column. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `A` | Number | The trapezoidal integral ∫ y dx. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `UNKNOWN_COLUMN` | `'y'` or `'x'` not a column | Use valid column names from the result table. |
