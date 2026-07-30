---
name: if
category: Logic
summary: Inline three-way conditional based on comparing a and b.
related: [min, max, sign, step]
examples: [sounding-rocket-trajectory]
tags: [logic, conditional, branch, if, comparison]
references: []
---

# if

Returns one of three values depending on how `a` compares to `b`: `lt` if `a < b`,
`eq` if `a = b`, `gt` if `a > b`. It is the inline branch for the declarative top
level (use `IF…THEN` inside `FUNCTION`/`PROCEDURE` bodies).

## Syntax

```
y = If(a, b, lt, eq, gt)
```

## Description

A smooth-free conditional select. Because frees is an equation solver, `If` lets a
value switch on a comparison without introducing imperative control flow into the
document body.

## Mathematical Formulation

$$ y = \begin{cases} lt & a < b \\ eq & a = b \\ gt & a > b \end{cases} $$

## Examples

### Example 1 — Phase switch in a rocket trajectory

[Run: sounding-rocket-trajectory]

**Expected:** the conditional selects the active branch (e.g. powered vs coast)
based on the comparison.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `a` | Number | Yes | Left comparison operand. |
| `b` | Number | Yes | Right comparison operand. |
| `lt` | Number | Yes | Value returned when `a < b`. |
| `eq` | Number | Yes | Value returned when `a = b`. |
| `gt` | Number | Yes | Value returned when `a > b`. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | The selected branch value. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `UNIT_MISMATCH` | `a` and `b` have incompatible units | Compare quantities with compatible dimensions. |
