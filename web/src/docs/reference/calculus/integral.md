---
name: integral
category: Calculus
summary: Definite integral; with self-reference, a scalar first-order ODE.
related: [gaussintegral, differentiate, IntegralValue]
examples: [tank-draining, newton-cooling]
tags: [calculus, integral, quadrature, ode, definite integral]
---

# integral

Computes a **definite integral** of an expression with respect to a variable over a
range. When the integrand references the result variable itself, frees detects the
self-reference and integrates the corresponding **first-order initial-value ODE**
starting from 0 at the lower limit.

## Syntax

```
A = Integral(expr, var, lower, upper)
```

## Description

For a plain definite integral, `expr` depends only on `var`. For the ODE pattern,
`expr` contains the result variable — frees then integrates the *change*, so you
rebuild the quantity of interest from the integrated increment. For coupled,
stiff, or multi-state systems use a `DYNAMIC` block instead.

## Mathematical Formulation

$$ A = \int_{\text{lower}}^{\text{upper}} \text{expr}(\text{var})\,d(\text{var}) $$

In the self-referential (ODE) form, with `y` the result and `y(lower) = 0`,

$$ \frac{dy}{d\,\text{var}} = \text{expr}(y, \text{var}), \qquad y = \int_{\text{lower}}^{\text{var}} \text{expr}\,d(\text{var}) $$

> **Method:** adaptive quadrature for a definite integral; an initial-value ODE
> integration when self-reference is detected.

## Examples

### Example 1 — Draining-tank ODE via the self-reference pattern

[Run: tank-draining]

**Expected:** integrating the volume drop and rebuilding `V = V0 − drop` gives the
tank volume after the elapsed time.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `expr` | Expression | Yes | Integrand (may self-reference the result for the ODE form). |
| `var` | Variable | Yes | Integration variable. |
| `lower` | Number | Yes | Lower limit. |
| `upper` | Number | Yes | Upper limit. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `A` | Number | The definite integral (or integrated change for the ODE form). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `NON_CONVERGENT` | integrand singular/stiff | Use `GaussIntegral`, a finer setup, or a `DYNAMIC` block. |
