---
name: eos_density
category: Properties (EOS)
summary: Mass density from a cubic equation of state (SRK or PR).
related: [eos_z, eos_volume, eos_enthalpy]
examples: [cubic-eos-properties]
tags: [eos, cubic, peng-robinson, srk, density, real gas]
---

# eos_density

Returns the **mass density** `ρ` [kg/m³] of a real fluid from a cubic equation of
state (`'SRK'` or `'PR'`) at temperature `T` and pressure `P`. The real-gas density that needs no
real-fluid property data, the reciprocal of `eos_volume`.

## Syntax

```
rho = eos_density(fluid$, model$, T, P, phase$)
```

## Description

The density follows from the EOS compressibility factor; near the critical region
it deviates strongly from the ideal-gas value `P/(RT)`.

## Mathematical Formulation

$$ \rho = \frac{1}{v} = \frac{P}{Z\,R\,T} $$

with `Z` the EOS root for the requested phase and `R` the specific gas constant.

> **Method:** `eos_z` root → `ρ = P/(ZRT)`.

## Examples

### Example 1 — CO₂ density near the critical point

[Run: cubic-eos-properties]

**Expected (approx.):** at 6 MPa, 320 K (PR), `ρ ≈ 140 kg/m³` (`Z ≈ 0.7`) — far
above the ideal-gas estimate because `Z < 1`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `fluid$` | String | Yes | Fluid name. |
| `model$` | String | Yes | `'SRK'` or `'PR'`. |
| `T` | Number | Yes | Temperature [K]. |
| `P` | Number | Yes | Pressure [Pa]. |
| `phase$` | String | Yes | Root selector: `'vapor'` or `'liquid'`. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `rho` | Number | Mass density [kg/m³]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `UNKNOWN_FLUID` | `fluid$` not in the table | Use a supported fluid name. |
