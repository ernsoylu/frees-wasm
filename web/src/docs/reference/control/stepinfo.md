---
name: stepinfo
category: Control Systems
summary: Step-response performance metrics (rise time, peak time, settling time, overshoot).
related: [step, pole, margin]
examples: []
tags: [control, step response, rise time, settling time, overshoot, transient]
---

# stepinfo

Returns the **transient performance metrics** of a step response sampled as
`(t, y)`: rise time `Tr`, peak time `Tp`, settling time `Ts`, and percent overshoot
`OS`. Use it to quantify a closed-loop design against time-domain specifications.

## Syntax

```
CALL stepinfo(t, y : Tr, Tp, Ts, OS)
[Tr, Tp, Ts, OS] = stepinfo(t, y)
```

## Mathematical Formulation

From the response `y(t)` with steady-state value `y_∞` and peak `y_p`:

$$ OS = \frac{y_p - y_\infty}{y_\infty}\times 100\%, \qquad T_p = \arg\max_t y(t) $$

`Tr` is the 10–90% rise time and `Ts` the time after which `|y − y_∞|` stays within
a 2% band.

> **Method:** scan the response for the crossing, peak, and settling instants.

## Examples

```
{ [Tr, Tp, Ts, OS] = stepinfo(t, y) from a step() response }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `t` | Vector | Yes | Time samples [s]. |
| `y` | Vector | Yes | Step response aligned with `t`. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `Tr` | Number | Rise time (10–90%) [s]. |
| `Tp` | Number | Peak time [s]. |
| `Ts` | Number | Settling time (2% band) [s]. |
| `OS` | Number | Percent overshoot. |
