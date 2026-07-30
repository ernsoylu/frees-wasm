---
name: erf
category: Special Functions
summary: Error function erf(x).
related: [erfc, erfinv, normalcdf]
examples: []
tags: [special function, error function, erf, gaussian, probability]
references:
  - "NIST Digital Library of Mathematical Functions, §7.2"
---

# erf

Returns the **error function** `erf(x)` — the scaled integral of the Gaussian. It
underlies the normal distribution, diffusion, and transient-conduction solutions.

## Syntax

```
y = erf(x)
```

## Description

An odd function (`erf(−x) = −erf(x)`) ranging from −1 to 1, with `erf(0) = 0` and
`erf(∞) = 1`.

## Mathematical Formulation

$$ \operatorname{erf}(x) = \frac{2}{\sqrt{\pi}}\int_0^x e^{-t^2}\,dt $$

related to the normal CDF by `Φ(x) = ½[1 + erf(x/√2)]`.

> **Method:** rational/continued-fraction approximation to machine precision.

## Examples

```
{ erf(1) ~ 0.8427 }
y = erf(1)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Real argument. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `y` | Number | erf(x) ∈ (−1, 1). |

## References

1. NIST *Digital Library of Mathematical Functions*, §7.2.
