---
name: d2c
category: Control Systems
summary: Discrete-to-continuous transfer-function conversion (Tustin / ZOH).
related: [c2d, tf, pole]
examples: []
tags: [control, discretization, d2c, tustin, zoh, continuous]
---

# d2c

Converts a discrete transfer function `G(z) = numz/denz` to a continuous-time
equivalent `G(s) = num/den` at sample time `Ts` — the inverse of `c2d`,
using the requested method (`'tustin'` bilinear or `'zoh'`).

## Syntax

```
CALL d2c(numz, denz, Ts, 'tustin' : num, den)
[num, den] = d2c(numz, denz, Ts, 'zoh')
```

## Description

Use it to recover a continuous model from an identified or implemented discrete
controller for analysis in the s-domain.

## Mathematical Formulation

Tustin (bilinear), the inverse of the `c2d` substitution:

$$ G(s) = G(z)\Big|_{\,z = \frac{1 + (T_s/2)s}{1 - (T_s/2)s}} $$

> **Method:** inverse bilinear substitution (or inverse ZOH).

## Examples

```
{ [num, den] = d2c(numz, denz, Ts, 'tustin') }
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `numz` | Vector | Yes | Discrete numerator (descending powers of `z`). |
| `denz` | Vector | Yes | Discrete denominator. |
| `Ts` | Number | Yes | Sample time [s]. |
| `method$` | String | Yes | `'tustin'` or `'zoh'`. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `num` | Vector | Continuous numerator (descending powers of `s`). |
| `den` | Vector | Continuous denominator. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `BAD_SAMPLE_TIME` | `Ts ≤ 0` | Use a positive sample time. |
