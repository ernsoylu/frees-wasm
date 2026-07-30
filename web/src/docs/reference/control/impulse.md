---
name: impulse
category: Control Systems
summary: Impulse response of a transfer function over a time vector.
related: [step, lsim, pole]
examples: [step-impulse-response]
tags: [control, impulse response, transient, time domain]
---

# impulse

Returns the **impulse response** `y(t)` of `G(s) = num/den` sampled at the times in
`t` — the system output to a unit impulse input. It is the inverse Laplace
transform of `G(s)` itself and the kernel of the convolution that gives any
response.

## Syntax

```
CALL impulse(num, den, t : y)
y = impulse(num, den, t)
```

## Description

The impulse response characterizes the system's natural modes directly; it is also
the derivative of the step response.

## Mathematical Formulation

$$ y(t) = \mathcal{L}^{-1}\{G(s)\}, \qquad g(t) = \frac{d}{dt}\,y_{\text{step}}(t) $$

> **Method:** numerical evaluation of the impulse response at each `t`.

## Examples

### Example 1 — Impulse response of a plant

[Run: step-impulse-response]

**Expected:** a decaying (and, for complex poles, oscillating) response returning to
zero, reflecting the open-loop poles.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `num` | Vector | Yes | Numerator coefficients (descending powers of `s`). |
| `den` | Vector | Yes | Denominator coefficients (descending powers of `s`). |
| `t` | Vector | Yes | Time samples [s]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Vector | Impulse response at each time. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `IMPROPER_TF` | `num` order exceeds `den` order | Provide a proper transfer function. |
