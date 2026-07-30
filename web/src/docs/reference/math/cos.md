---
name: cos
category: Math
summary: Cosine of an angle (radians).
related: [sin, tan, arccos, atan2]
examples: [projectile-motion, projectile-trajectory]
tags: [math, trigonometry, cosine, radians, elementary]
references: []
---

# cos

Returns the **cosine** of `x`. The argument is in **radians** unless it carries a
`[deg]` unit annotation (which frees converts automatically).

## Syntax

```
y = cos(x)
```

## Description

A standard trigonometric function. Use `x [deg]` or `Convert` to work in degrees.

## Mathematical Formulation

$$ y = \cos(x), \qquad x \text{ in radians} $$

## Examples

### Example 1 — Launch-angle component of velocity

[Run: projectile-motion]

**Expected:** `cos` of the launch angle gives the horizontal velocity component.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Angle in radians (or `[deg]`-annotated). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | Cosine of `x` (dimensionless, in [−1, 1]). |
