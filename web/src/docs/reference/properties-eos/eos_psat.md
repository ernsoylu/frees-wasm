---
name: eos_psat
category: Properties (EOS)
summary: Saturation pressure from a cubic EOS via the equal-fugacity condition.
related: [eos_z, eos_enthalpy]
examples: [cubic-eos-properties]
tags: [eos, cubic, peng-robinson, srk, saturation pressure, fugacity, vapor pressure]
---

# eos_psat

Returns the **saturation (vapor) pressure** `Psat` [Pa] of a fluid at temperature
`T` from a cubic equation of state (`'SRK'` or `'PR'`), found by enforcing
equal liquid and vapor fugacities (the phase-equilibrium condition).

## Syntax

```
Psat = eos_psat(fluid$, model$, T)
```

## Description

At a given `T`, the saturation pressure is the unique pressure at which the EOS
admits coexisting liquid and vapor roots with equal fugacity — the cubic-EOS
analogue of the Maxwell equal-area construction.

## Mathematical Formulation

`Psat(T)` is the pressure satisfying the isofugacity condition

$$ f^{\,L}(T, P_{sat}) = f^{\,V}(T, P_{sat}) \quad\Longleftrightarrow\quad \varphi^{L} = \varphi^{V} $$

where the fugacity coefficient from the cubic EOS is

$$ \ln\varphi = (Z-1) - \ln(Z-B) - \frac{A}{2\sqrt2\,B}\ln\!\left[\frac{Z+(1+\sqrt2)B}{Z+(1-\sqrt2)B}\right] $$

(Peng–Robinson). The liquid and vapor `Z` roots are evaluated at trial `P` and the
pressure is iterated until $\varphi^L = \varphi^V$.

> **Method:** root-find on `P` so that the liquid/vapor fugacity coefficients match.

## Examples

### Example 1 — CO₂ vapor pressure at 300 K

[Run: cubic-eos-properties]

**Expected:** `eos_psat('co2', 'PR', 300) ≈ 6.7 MPa` (CO₂ vapor pressure at 300 K;
the critical point is 304 K / 7.38 MPa).

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `fluid$` | String | Yes | Fluid name. |
| `model$` | String | Yes | `'SRK'` or `'PR'`. |
| `T` | Number | Yes | Temperature [K] (below the critical temperature). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `Psat` | Number | Saturation pressure [Pa]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `SUPERCRITICAL` | `T ≥ Tc` | No saturation pressure exists above the critical temperature. |
| `UNKNOWN_FLUID` | `fluid$` not in the table | Use a supported fluid name. |
