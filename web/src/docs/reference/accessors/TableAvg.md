---
name: TableAvg
category: Tables
summary: Average of a column across the parametric table runs.
related: [TableSum, TableMin, TableMax, TableStdDev]
examples: [driving-cycle-energy]
tags: [accessor, parametric table, average, mean, column]
references: []
---

# TableAvg

Returns the **arithmetic mean** of a named column across all rows of the
parametric table. Use it to summarize a swept study — e.g. the average consumption
over the points of a drive cycle.

## Syntax

```
m = TableAvg('col')
```

## Description

After a parametric (swept) solve, each variable becomes a column with one value per
run. `TableAvg` averages the requested column over all runs.

## Mathematical Formulation

For a column with values $c_1, \dots, c_n$ over `n` runs,

$$ \text{TableAvg}('col') = \frac{1}{n}\sum_{i=1}^{n} c_i $$

> **Method:** arithmetic mean over the parametric-table rows.

## Examples

### Example 1 — Average over a drive-cycle sweep

[Run: driving-cycle-energy]

**Expected:** the mean of the requested column across the table's runs.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'col'` | String | Yes | Name of a parametric-table column. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `m` | Number | Mean of the column across all runs. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `UNKNOWN_COLUMN` | `'col'` not a table column | Use a variable present in the parametric table. |
| `NO_TABLE` | No parametric table has been solved | Run the parametric table first (Solve Table). |
