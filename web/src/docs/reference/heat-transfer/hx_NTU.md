---
name: hx_NTU
category: Heat Transfer
summary: Number of transfer units NTU(ε, Cr) — the inverse of hx_effectiveness.
related: [hx_effectiveness, LMTD]
examples: [hx-effectiveness-ntu]
tags: [heat exchanger, ntu, effectiveness, sizing, counterflow]
---

# hx_NTU

Returns the **number of transfer units** `NTU` required to achieve a target
effectiveness `ε` at capacity ratio `Cr` for a given flow arrangement — the
inverse of `hx_effectiveness`. Use it to **size** an exchanger:
from a required duty you back out `ε`, then `NTU`, then `UA = NTU·C_min`.

## Syntax

```
NTU = hx_NTU(type$, eps, Cr)
```

## Description

Rating asks "what duty for this `UA`?" (effectiveness from NTU); sizing asks the
reverse — "what `UA` for this duty?". `hx_NTU` closes the sizing loop by inverting
the `ε`(NTU, Cr) relation analytically per arrangement.

## Mathematical Formulation

With $NTU = UA/C_{min}$ and $C_r = C_{min}/C_{max}$:

**Counterflow** ($C_r < 1$):

$$ NTU = \frac{1}{C_r - 1}\,\ln\!\left(\frac{\varepsilon - 1}{\varepsilon\,C_r - 1}\right) $$

**Counterflow, balanced** ($C_r = 1$):

$$ NTU = \frac{\varepsilon}{1 - \varepsilon} $$

**Condenser / evaporator limit** ($C_r \to 0$; arrangement-independent):

$$ NTU = -\ln(1 - \varepsilon) $$

> **Method:** direct evaluation of the inverse relation for the selected
> arrangement; the $C_r \to 0$ form is used for condensers/evaporators.

## Examples

### Example 1 — NTU of a counterflow exchanger

The effectiveness–NTU rating example forms `NTU = UA/C_min` directly; `hx_NTU`
performs the reverse map `ε → NTU` for the same arrangement when sizing to a
target effectiveness.

[Run: hx-effectiveness-ntu]

**Expected:** for `ε ≈ 0.711`, `Cr = 0.75`, `hx_NTU('counterflow', 0.711, 0.75) ≈ 1.91`
(recovering the example's `NTU = UA/C_min`).

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `type$` | String | Yes | Flow arrangement: `'counterflow'`, `'parallel'`. A condenser/evaporator is the `Cr = 0` case. |
| `eps` | Number | Yes | Target effectiveness ε ∈ [0, 1). Must satisfy `ε < 1/(1+Cr)` for parallel flow. |
| `Cr` | Number | Yes | Capacity ratio `C_min/C_max` ∈ [0, 1]. Use `0` for a condenser/evaporator. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `NTU` | Number | Number of transfer units (≥ 0); `UA = NTU·C_min`. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `INFEASIBLE_EFFECTIVENESS` | `ε` at or above the arrangement's ceiling (e.g. `ε ≥ 1/(1+Cr)` for parallel flow) | No finite `NTU` reaches it — lower the target or switch to counterflow. |
| `UNKNOWN_HX_TYPE` | `type$` not recognized | Use `'counterflow'` or `'parallel'`; pass `Cr = 0` for a condenser/evaporator. |
