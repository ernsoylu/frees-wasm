---
name: lsim
category: Control Systems
summary: Response of a transfer function to an arbitrary input u(t).
related: [step, impulse]
examples: [step-impulse-response]
tags: [control, simulation, arbitrary input, convolution, time domain]
---

# lsim

Returns the **time response** `y(t)` of `G(s) = num/den` to an **arbitrary input**
`u(t)` sampled on the time vector `t`. Use it to simulate a system under a custom
forcing (ramps, pulses, measured signals) rather than the canned step/impulse.

## Syntax

```
CALL lsim(num, den, u, t : y)
y = lsim(num, den, u, t)
```

## Description

`u` and `t` are aligned vectors describing the input over time; `y` is the
corresponding output. The response is the convolution of the input with the
system's impulse response.

## Mathematical Formulation

$$ y(t) = \int_0^t g(t-\tau)\,u(\tau)\,d\tau, \qquad g(t) = \mathcal{L}^{-1}\{G(s)\} $$

> **Method:** numerical convolution / state-space integration of the input over `t`.

## Examples

### Example 1 — Response to a custom input

[Run: step-impulse-response]

**Expected:** the output tracking the supplied input, shaped by the plant dynamics.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `num` | Vector | Yes | Numerator coefficients (descending powers of `s`). |
| `den` | Vector | Yes | Denominator coefficients (descending powers of `s`). |
| `u` | Vector | Yes | Input samples aligned with `t`. |
| `t` | Vector | Yes | Time samples [s]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Vector | Output response at each time. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `LENGTH_MISMATCH` | `u` and `t` differ in length | Provide input and time vectors of equal length. |
