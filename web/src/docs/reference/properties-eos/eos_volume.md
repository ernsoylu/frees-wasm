---
name: eos_volume
category: Properties (EOS)
summary: Specific volume from a cubic equation of state (SRK or PR).
related: [eos_z, eos_density, eos_pressure]
examples: [cubic-eos-properties]
tags: [eos, cubic, peng-robinson, srk, specific volume, real gas]
---

# eos_volume

Returns the **specific volume** `v` [m³/kg] of a real fluid from a cubic equation
of state (`'SRK'` or `'PR'`) at temperature `T` and pressure `P`. It is the
compressibility factor expressed as a volume — the reciprocal of
`eos_density`.

## Syntax

```
v = eos_volume(fluid$, model$, T, P, phase$)
```

## Description

Once the cubic is solved for the compressibility factor `Z` (see `eos_z`),
the specific volume follows directly from its definition.

## Mathematical Formulation

$$ v = \frac{Z\,R\,T}{P} $$

where `R` is the specific gas constant of the fluid and `Z` is the EOS root for the
requested phase.

> **Method:** `eos_z` root → `v = ZRT/P`.

## Examples

### Example 1 — CO₂ specific volume

[Run: cubic-eos-properties]

**Expected (approx.):** at 6 MPa, 320 K (PR), `v ≈ 7×10⁻³ m³/kg` (`Z ≈ 0.7`),
the reciprocal of the density.

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
| `v` | Number | Specific volume [m³/kg]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `UNKNOWN_FLUID` | `fluid$` not in the table | Use a supported fluid name. |
