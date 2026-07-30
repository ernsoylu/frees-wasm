---
name: erfinv
category: Special Functions
summary: Inverse error function erf⁻¹(x).
related: [erf, erfc, normalinvcdf]
examples: []
tags: [special function, inverse error function, erfinv, quantile, gaussian]
references:
  - "NIST Digital Library of Mathematical Functions, §7.17"
---

# erfinv

Returns the **inverse error function** `erf⁻¹(x)` — the value `w` such that
`erf(w) = x`. It maps a probability-like value back to a Gaussian deviate and
underlies normal-quantile (inverse-CDF) calculations.

## Syntax

```
w = erfinv(x)
```

## Description

Defined on `−1 < x < 1`; it diverges as `x → ±1`. An odd function.

## Mathematical Formulation

$$ w = \operatorname{erf}^{-1}(x) \quad\Longleftrightarrow\quad \operatorname{erf}(w) = x, \qquad -1 < x < 1 $$

linked to the normal quantile by `Φ⁻¹(p) = √2·erfinv(2p − 1)`.

> **Method:** rational approximation refined by Newton iteration on `erf`.

## Examples

```
{ erfinv(0.8427) ~ 1.0 }
w = erfinv(0.8427)
```

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `x` | Number | Yes | Value in (−1, 1). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `w` | Number | erf⁻¹(x). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `|x| ≥ 1` | The argument must lie strictly in (−1, 1). |

## References

1. NIST *Digital Library of Mathematical Functions*, §7.17.
