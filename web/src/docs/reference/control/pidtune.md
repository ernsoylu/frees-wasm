---
name: pidtune
category: Control Systems
summary: Automatic PID gain tuning by loop-shaping to a target crossover.
related: [margin, feedback, lqr]
examples: [controller-design-lqr-pid]
tags: [control, pid, tuning, loop shaping, crossover, kp ki kd]
---

# pidtune

Returns tuned **PID gains** `Kp`, `Ki`, `Kd` for a plant `G(s) = num/den`, designed
to achieve a target gain-crossover frequency `wc` with adequate phase margin. Use it
for a quick, systematic classical controller without manual loop shaping.

## Syntax

```
CALL pidtune(num, den, 'PID', wc : Kp, Ki, Kd)
[Kp, Ki, Kd] = pidtune(num, den, 'PID', wc)
```

## Description

The controller `C(s) = Kp + Ki/s + Kd·s` is shaped so the open loop `C·G` crosses
0 dB near `wc` with a phase margin that yields a well-damped closed loop. The type
string selects `'P'`, `'PI'`, `'PD'`, or `'PID'`.

## Mathematical Formulation

$$ C(s) = K_p + \frac{K_i}{s} + K_d\,s $$

The gains are chosen so that at the target crossover `ωc`:

$$ |C(j\omega_c)G(j\omega_c)| = 1, \qquad \angle C(j\omega_c)G(j\omega_c) = -180° + \text{PM} $$

> **Method:** solve the magnitude/phase loop-shaping conditions at `wc` for the
> controller gains.

## Examples

### Example 1 — Tune a PID controller

[Run: controller-design-lqr-pid]

**Expected:** `Kp`, `Ki`, `Kd` giving a stable closed loop with the targeted
crossover and margin.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `num` | Vector | Yes | Plant numerator (descending powers of `s`). |
| `den` | Vector | Yes | Plant denominator. |
| `type$` | String | Yes | `'P'`, `'PI'`, `'PD'`, or `'PID'`. |
| `wc` | Number | Yes | Target gain-crossover frequency [rad/s]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `Kp` | Number | Proportional gain. |
| `Ki` | Number | Integral gain. |
| `Kd` | Number | Derivative gain. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `INFEASIBLE_WC` | target crossover unreachable | Choose a `wc` consistent with the plant bandwidth. |
