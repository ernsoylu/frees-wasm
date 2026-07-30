---
name: bode
category: Control Systems
summary: Bode frequency response — magnitude (dB) and phase (deg) versus frequency.
related: [nyquist, margin]
examples: [control-analysis-report]
tags: [control, bode, frequency response, magnitude, phase, frequency]
---

# bode

Returns the **Bode frequency response** of `G(s) = num/den` over a frequency
vector `omega`: magnitude in decibels and phase in degrees. Use it to read
bandwidth, resonance, roll-off, and the stability margins.

## Syntax

```
CALL bode(num, den, omega : mag, phase)
[mag, phase] = bode(num, den, omega)
```

## Description

Evaluating the transfer function on the imaginary axis `s = jω` gives the
steady-state response to a sinusoid at each frequency. `mag` and `phase` are
vectors aligned with `omega` (typically log-spaced).

## Mathematical Formulation

$$ \text{mag}(\omega) = 20\log_{10}\big|G(j\omega)\big| \quad[\text{dB}], \qquad \text{phase}(\omega) = \angle G(j\omega) \quad[\text{deg}] $$

> **Method:** evaluate `G(jω)` at each `omega`; magnitude in dB, phase unwrapped in
> degrees.

## Examples

### Example 1 — Bode response of a second-order plant

50 log-spaced frequencies over `G(s) = (s + 2)/(s² + 4s + 25)`:

[Run: control-analysis-report]

**Expected:** a resonant rise near `ω_n = 5 rad/s` (ζ ≈ 0.4) and a high-frequency
roll-off of −20 dB/decade (one more pole than zero).

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `num` | Vector | Yes | Numerator coefficients (descending powers of `s`). |
| `den` | Vector | Yes | Denominator coefficients (descending powers of `s`). |
| `omega` | Vector | Yes | Frequencies [rad/s] (usually log-spaced). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `mag` | Vector | Magnitude [dB] at each frequency. |
| `phase` | Vector | Phase [deg] at each frequency. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `EMPTY_FREQUENCY` | `omega` is empty | Provide a frequency vector, e.g. `omega = 0.1:50:100 | Log`. |
