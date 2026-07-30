---
name: dtable1
category: Interpolation
summary: Cubic-spline derivative of a table at x — the smooth d/dx of Interpolate1.
related: [dtable, interpolate1, differentiate]
examples: []
tags: [table, derivative, spline, cubic, interpolation, smooth]
---

# dtable1

Returns the derivative of the **natural cubic spline** through the table's first
curve at `x` — the smooth counterpart of `dtable`, matching the interpolant
`Interpolate1` evaluates. Use it when the consumer differentiates again (the
linear interpolant's slope is discontinuous at knots) or when the tabulated data
represents a smooth underlying function.

## Syntax

```
d = dtable1('t', x)
```

## Description

The spline is built over the sorted x column against the first y curve; `x`
clamps to the tabulated range. Tables with fewer than three rows fall back to
the linear-segment slope (same as `dtable`).

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'t'` | String | Yes | Name of a `TABLE` block (or a bound `map$` parameter). |
| `x` | Number | Yes | Evaluation point, in the table's x units. |

## Examples

### Example 1 — spline slope through y = x²

```
TABLE quad(x)
  0   0
  1   1
  2   4
END
s = dtable1('quad', 1)    { = 2 — the natural spline through x² has the exact slope at the middle knot }
```

## See also

`dtable`, `Interpolate1`, `Differentiate`
