---
name: AdiabaticFlameTemp
category: Combustion
summary: Constant-pressure adiabatic flame temperature of a fuel-air mixture.
related: [AdiabaticFlameTempEq, mix_enthalpy, wiebe_rate]
examples: [adiabatic-flame-temp]
tags: [combustion, adiabatic flame temperature, energy balance, equivalence ratio, methane]
---

# AdiabaticFlameTemp

Returns the **constant-pressure adiabatic flame temperature** `T_ad` [K] of a fuel
burning in air at equivalence ratio `phi`, with reactants entering at `T_react`.
It is the temperature the products reach when all the combustion energy goes into
heating them (no heat loss, no work). This variant uses **frozen** complete
products (no dissociation) — see `AdiabaticFlameTempEq` for the dissociating case.

## Syntax

```
T_ad = AdiabaticFlameTemp(fuel$, phi, T_react)
```

## Description

`phi = 1` is stoichiometric; `phi < 1` is lean (excess air, which dilutes and
lowers the flame temperature). The result is the upper bound on combustor
temperature for the given mixture.

## Mathematical Formulation

For an adiabatic, constant-pressure combustor with no work, the first law reduces
to equal reactant and product enthalpies — solve for `T_ad`:

$$ H_{\text{react}}(T_{\text{react}}) = H_{\text{prod}}(T_{\text{ad}}) $$

i.e.

$$ \sum_{\text{react}} N_i\big(\bar h_f^\circ + \Delta\bar h(T_{\text{react}})\big)_i = \sum_{\text{prod}} N_j\big(\bar h_f^\circ + \Delta\bar h(T_{\text{ad}})\big)_j $$

where $\bar h_f^\circ$ is the enthalpy of formation and $\Delta\bar h(T)$ the
sensible enthalpy (here from NASA-7 polynomials).

> **Method:** root-find `T_ad` so the product enthalpy matches the reactant
> enthalpy; products are the complete (frozen) combustion species.

## Examples

### Example 1 — Stoichiometric methane–air flame

[Run: adiabatic-flame-temp]

**Expected (approx.):** for `CH4`, `phi = 1`, reactants at 298.15 K,
`T_flame ≈ 2300 K` (frozen products; dissociation would lower this by ~100–150 K).

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `fuel$` | String | Yes | Fuel name/formula (e.g. `'CH4'`). |
| `phi` | Number | Yes | Equivalence ratio (1 = stoichiometric, < 1 = lean). |
| `T_react` | Number | Yes | Reactant inlet temperature [K]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `T_ad` | Number | Adiabatic flame temperature [K]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `UNKNOWN_FUEL` | `fuel$` has no thermo data | Use a fuel present in the NASA-7 species set. |
