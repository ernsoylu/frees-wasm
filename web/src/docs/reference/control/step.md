---
name: step
category: Control Systems
summary: Unit step response of a transfer function over a time vector.
related: [impulse, lsim, pole]
examples: [control-analysis-report]
tags: [control, step response, transient, overshoot, time domain]
---

# step

Returns the **unit step response** `y(t)` of `G(s) = num/den` sampled at the times
in `t`. Use it to read transient metrics — rise time, peak overshoot, settling
time — directly from the time-domain response.

## Syntax

```
CALL step(num, den, t : y)
y = step(num, den, t)
```

## Description

The step response is the inverse Laplace transform of `G(s)/s` (a unit step input
`U(s) = 1/s`). For an underdamped second-order system it exhibits the familiar
overshoot-and-ring governed by the damping ratio and natural frequency.

## Mathematical Formulation

$$ Y(s) = G(s)\cdot\frac{1}{s}, \qquad y(t) = \mathcal{L}^{-1}\{Y(s)\} $$

For a standard second-order system $G = \omega_n^2/(s^2 + 2\zeta\omega_n s + \omega_n^2)$,
the peak overshoot is $M_p = \exp\!\big(-\pi\zeta/\sqrt{1-\zeta^2}\big)$.

> **Method:** numerical evaluation of the step response at each `t`.

## Examples

### Example 1 — Step response of an underdamped plant

`G(s) = (s + 2)/(s² + 4s + 25)`, integrated over 4 s:

[Run: control-analysis-report]

**Expected:** an overshooting, ringing response (ζ ≈ 0.4) that settles toward its
steady-state value within a few seconds.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `num` | Vector | Yes | Numerator coefficients (descending powers of `s`). |
| `den` | Vector | Yes | Denominator coefficients (descending powers of `s`). |
| `t` | Vector | Yes | Time samples [s]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Vector | Step response `y(t)` at each time. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `IMPROPER_TF` | `num` higher order than `den` | The transfer function must be proper (order of `num` ≤ order of `den`). |
