---
name: StagnationTemp
category: Compressible Flow
summary: Stagnation temperature T0 = T + V²/(2·cp).
related: [StagnationPres, T0_T]
examples: [thermo-compliance]
tags: [compressible, stagnation temperature, total temperature, energy]
---

# StagnationTemp

Returns the **stagnation (total) temperature** `T0` of a flowing gas — the
temperature it would reach if brought adiabatically to rest — from the static
temperature `T`, velocity `V`, and specific heat `cp`.

## Syntax

```
T0 = StagnationTemp(T, V, cp)
```

## Description

The stagnation temperature adds the kinetic-energy contribution of the flow to the
static temperature. It is conserved along an adiabatic flow even as static
conditions change.

## Mathematical Formulation

$$ T_0 = T + \frac{V^2}{2\,c_p} $$

> **Method:** direct evaluation of the energy balance.

## Examples

### Example 1 — Total temperature of a flow

[Run: thermo-compliance]

**Expected:** `T0 > T`, the excess set by the kinetic term `V²/(2cp)`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `T` | Number | Yes | Static temperature [K]. |
| `V` | Number | Yes | Flow velocity [m/s]. |
| `cp` | Number | Yes | Specific heat at constant pressure [J/kg·K]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `T0` | Number | Stagnation temperature [K]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `cp ≤ 0` | Provide a positive specific heat. |
