---
name: eos_z
category: Properties (EOS)
summary: Compressibility factor Z from a cubic equation of state (SRK or PR).
related: [eos_volume, eos_density, eos_enthalpy, eos_psat]
examples: [cubic-eos-properties]
tags: [eos, cubic, peng-robinson, srk, compressibility, real gas, z-factor]
---

# eos_z

Returns the **compressibility factor** `Z = Pv/(RT)` of a real fluid from a cubic
equation of state — Soave–Redlich–Kwong (`'SRK'`) or Peng–Robinson (`'PR'`) — at
temperature `T` and pressure `P`. `Z` measures the departure from ideal-gas
behavior (`Z = 1`) and is the root used to build all other EOS properties. A
CoolProp-independent backend that needs only critical constants and the acentric
factor.

## Syntax

```
Z = eos_z(fluid$, model$, T, P, phase$)
```

## Description

`model$` selects `'SRK'` or `'PR'`; `phase$` (`'vapor'`/`'liquid'`) picks which
real root of the cubic to return when the EOS is multivalued (two-phase region).

## Mathematical Formulation

The Peng–Robinson equation of state,

$$ P = \frac{RT}{v-b} - \frac{a\,\alpha(T)}{v(v+b) + b(v-b)} $$

with $a = 0.45724\,R^2T_c^2/P_c$, $b = 0.07780\,RT_c/P_c$, and
$\alpha = [1 + m(1-\sqrt{T/T_r})]^2$, $m = 0.37464 + 1.54226\omega - 0.26992\omega^2$,
rearranges into the cubic in `Z` (with $A = a\alpha P/(RT)^2$, $B = bP/RT$):

$$ Z^3 - (1-B)Z^2 + (A - 2B - 3B^2)Z - (AB - B^2 - B^3) = 0 $$

(SRK uses $a = 0.42748\,R^2T_c^2/P_c$, $b = 0.08664\,RT_c/P_c$,
$m = 0.480 + 1.574\omega - 0.176\omega^2$ and the denominator $v(v+b)$.)

> **Method:** solve the cubic in `Z`; for a multivalued root, return the largest
> real root for `'vapor'` and the smallest for `'liquid'`.

## Examples

### Example 1 — CO₂ near the critical region

CO₂ at 6 MPa, 320 K with Peng–Robinson:

[Run: cubic-eos-properties]

**Expected (approx.):** `Z ≈ 0.7` — a strong real-gas deviation, since 320 K is
just above the CO₂ critical temperature (304 K) at near-critical pressure.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `fluid$` | String | Yes | Fluid name (critical constants + acentric factor looked up). |
| `model$` | String | Yes | `'SRK'` or `'PR'`. |
| `T` | Number | Yes | Temperature [K]. |
| `P` | Number | Yes | Pressure [Pa]. |
| `phase$` | String | Yes | Root selector: `'vapor'` or `'liquid'`. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `Z` | Number | Compressibility factor (dimensionless). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `UNKNOWN_FLUID` | `fluid$` not in the critical-constant table | Use a supported fluid name. |
| `UNKNOWN_MODEL` | `model$` not `'SRK'`/`'PR'` | Pass `'SRK'` or `'PR'`. |
