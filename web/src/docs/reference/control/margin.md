---
name: margin
category: Control Systems
summary: Gain and phase margins and their crossover frequencies.
related: [bode, nyquist, pole]
examples: [control-analysis-report]
tags: [control, gain margin, phase margin, stability, crossover, frequency response]
---

# margin

Returns the **gain margin** `gm`, **phase margin** `pm`, and the gain- and
phase-crossover frequencies (`w_cg`, `w_cp`) of an open-loop transfer function —
the classical frequency-domain measures of relative stability for the closed loop.

## Syntax

```
CALL margin(num, den : gm, pm, w_cg, w_cp)
[gm, pm, w_cg, w_cp] = margin(num, den)
```

## Description

Applied to the open-loop `L(s) = num/den`, the margins quantify how much
additional gain or phase lag the loop tolerates before the closed loop goes
unstable. Positive `gm` (in dB) and positive `pm` (in degrees) indicate a stable
closed loop.

## Mathematical Formulation

At the **phase-crossover** frequency $\omega_{cg}$ where $\angle L(j\omega_{cg}) = -180°$:

$$ GM = \frac{1}{|L(j\omega_{cg})|} \quad\text{(often in dB: } 20\log_{10} GM\text{)} $$

At the **gain-crossover** frequency $\omega_{cp}$ where $|L(j\omega_{cp})| = 1$:

$$ PM = 180° + \angle L(j\omega_{cp}) $$

> **Method:** locate the crossover frequencies on the open-loop frequency response,
> then evaluate the margins there.

## Examples

### Example 1 — Margins of a second-order plant

[Run: control-analysis-report]

**Expected:** with no `−180°` crossing, the gain margin is infinite and the phase
margin is large — consistent with the stable left-half-plane poles.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `num` | Vector | Yes | Open-loop numerator coefficients (descending powers of `s`). |
| `den` | Vector | Yes | Open-loop denominator coefficients (descending powers of `s`). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `gm` | Number | Gain margin (factor; report as `20·log10(gm)` dB). |
| `pm` | Number | Phase margin [deg]. |
| `w_cg` | Number | Gain-margin (phase-crossover) frequency [rad/s]. |
| `w_cp` | Number | Phase-margin (gain-crossover) frequency [rad/s]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `NO_CROSSOVER` | The response never crosses `−180°` or `0 dB` | Margin is infinite/undefined for this loop — interpret accordingly. |
