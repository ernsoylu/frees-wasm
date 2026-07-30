---
name: nyquist
category: Control Systems
summary: Nyquist frequency response — real and imaginary parts of G(jω).
related: [bode, margin]
examples: [control-analysis-report]
tags: [control, nyquist, frequency response, stability, polar plot]
---

# nyquist

Returns the **Nyquist (polar) frequency response** of `G(s) = num/den` over a
frequency vector `omega`: the real (`re`) and imaginary (`im`) parts of `G(jω)`.
Plotting `im` against `re` and applying the Nyquist criterion (encirclements of the
`−1 + j0` point) tests closed-loop stability.

## Syntax

```
CALL nyquist(num, den, omega : re, im)
[re, im] = nyquist(num, den, omega)
```

## Description

The Nyquist locus traces `G(jω)` in the complex plane as `ω` sweeps. Its proximity
to the critical point `−1 + j0` is the geometric basis of the gain and phase
margins.

## Mathematical Formulation

$$ G(j\omega) = \mathrm{re}(\omega) + j\,\mathrm{im}(\omega), \qquad \mathrm{re} = \Re\{G(j\omega)\},\ \ \mathrm{im} = \Im\{G(j\omega)\} $$

The Nyquist stability criterion relates closed-loop right-half-plane poles `Z` to
encirclements `N` of `−1` and open-loop RHP poles `P` by `Z = N + P`.

> **Method:** evaluate `G(jω)` at each `omega`; return Cartesian parts.

## Examples

### Example 1 — Nyquist locus of a second-order plant

[Run: control-analysis-report]

**Expected:** a locus that stays clear of `−1 + j0` (no encirclements), consistent
with the stable closed loop.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `num` | Vector | Yes | Numerator coefficients (descending powers of `s`). |
| `den` | Vector | Yes | Denominator coefficients (descending powers of `s`). |
| `omega` | Vector | Yes | Frequencies [rad/s]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `re` | Vector | Real part of `G(jω)` at each frequency. |
| `im` | Vector | Imaginary part of `G(jω)` at each frequency. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `EMPTY_FREQUENCY` | `omega` is empty | Provide a frequency vector spanning the dynamics of interest. |
