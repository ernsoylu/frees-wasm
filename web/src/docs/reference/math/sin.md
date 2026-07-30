---
name: sin
category: Math
summary: Sine of an angle (radians).
related: [cos, tan, arcsin, atan2]
examples: [projectile-motion, projectile-trajectory]
tags: [math, trigonometry, sine, radians, elementary]
references: []
---

# sin

Returns the **sine** of `x`. The argument is in **radians** unless it carries a
`[deg]` unit annotation (which frees converts automatically).

## Syntax

```
y = sin(x)
```

## Description

A standard trigonometric function. Use `x [deg]` or `Convert` to work in degrees;
bare numeric arguments are radians.

## Mathematical Formulation

$$ y = \sin(x), \qquad x \text{ in radians} $$

## Examples

### Example 1 — Launch-angle component of velocity

[Run: projectile-motion]

**Expected:** `sin` of the launch angle gives the vertical velocity component.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Angle in radians (or `[deg]`-annotated). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | Sine of `x` (dimensionless, in [−1, 1]). |
