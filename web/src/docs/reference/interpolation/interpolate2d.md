---
name: interpolate2d
category: Interpolation
summary: Bilinear interpolation of a 2-D table at (x, y).
related: [interpolate, lookup, interpolate1]
examples: [engine-map-2d]
tags: [interpolation, bilinear, 2d, table, lookup, map]
---

# interpolate2d

Performs **bilinear interpolation** of a named 2-D `TABLE` `t` at the point
`(x, y)`. Use it for engine maps, efficiency surfaces, and any quantity tabulated
against two independent variables.

## Syntax

```
z = Interpolate2D('t', x, y)
```

## Description

The table provides `z` on a grid of `x` (columns) and `y` (rows); the function
blends the four surrounding grid values weighted by the fractional position of
`(x, y)` within its cell.

## Mathematical Formulation

For `(x, y)` in the cell bounded by `x_i ≤ x ≤ x_{i+1}`, `y_j ≤ y ≤ y_{j+1}`, with
`t_x = (x − x_i)/(x_{i+1} − x_i)` and `t_y = (y − y_j)/(y_{j+1} − y_j)`:

$$ z = (1-t_x)(1-t_y)z_{i,j} + t_x(1-t_y)z_{i+1,j} + (1-t_x)t_y\,z_{i,j+1} + t_x t_y\,z_{i+1,j+1} $$

> **Method:** locate the bounding grid cell, then bilinear blend the four corners.

## Examples

### Example 1 — Engine efficiency from a 2-D map

[Run: engine-map-2d]

**Expected:** the interpolated map value at the requested speed/load point.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'t'` | String | Yes | Name of a 2-D `TABLE` block. |
| `x` | Number | Yes | First (column) coordinate. |
| `y` | Number | Yes | Second (row) coordinate. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `z` | Number | Bilinearly interpolated table value. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `OUT_OF_RANGE` | `(x, y)` outside the table grid | Keep the query within the tabulated bounds. |
| `UNKNOWN_TABLE` | `'t'` is not a defined table | Define the 2-D `TABLE` block first. |
