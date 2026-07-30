---
name: dtable
category: Interpolation
summary: Analytic slope of a table's linear interpolant — the exact derivative of t(x).
related: [dtable1, interpolate, differentiate]
examples: []
tags: [table, derivative, slope, interpolation, cam, feedforward, map]
---

# dtable

Returns the **exact derivative of the interpolant** a bare table call evaluates: for
`t(x)` (piecewise-linear), `dtable('t', x)` is the slope of the segment containing
`x`. Unlike `Differentiate` (a general column-vs-column numerical derivative), the
first y-curve against the x column is implied — the 1-D map-call convention.

## Syntax

```
d = dtable('t', x)
```

Inside a component, a `map$`-style string parameter works directly — this is the
feedforward/cam idiom the function exists for:

```
COMPONENT CamFollower(shaft, rod)
  PARAM prof$
  lift    = prof$(theta)
  rod.vel = dtable(prof$, theta) * shaft.w   { chain rule: dl/dθ · dθ/dt }
  ...
END
```

## Description

Because the slope is read from the interpolant itself (not finite-differenced),
it is exact everywhere for the linear interpolant — including between knots — and
piecewise-constant across a segment. At a knot the right-segment slope is
returned; outside the tabulated range the edge segment's slope extends. For a
smooth derivative use `dtable1` (cubic spline).

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `'t'` | String | Yes | Name of a `TABLE` block (or a bound `map$` parameter). |
| `x` | Number | Yes | Evaluation point, in the table's x units. |

## Examples

### Example 1 — exact slope of a linear table

```
TABLE lin(x)
  0   0
  1   3
  2   6
END
s = dtable('lin', 0.5)    { = 3, exactly }
```

## See also

`dtable1`, `Interpolate`, `Differentiate`
