---
name: mach_A_Astar
category: Compressible Flow
summary: Mach number from the isentropic area ratio A/A* and flow regime.
related: [T0_T, P0_P, M2_shock]
examples: [cd-nozzle-shock]
tags: [compressible, isentropic, area ratio, mach, nozzle, subsonic, supersonic]
---

# mach_A_Astar

Returns the **Mach number** at a duct station from its isentropic **area ratio**
`A/A*` and a regime selector. Because the area ratio is double-valued, `regime$`
picks the `'subsonic'` or `'supersonic'` root — essential for locating the state
in a converging–diverging nozzle.

## Syntax

```
M = mach_A_Astar(A_Astar, k, regime$)
```

## Description

For isentropic flow, each area ratio `A/A* ≥ 1` corresponds to one subsonic and
one supersonic Mach number (the throat is `A/A* = 1`, `M = 1`). This function
inverts the area–Mach relation for the requested branch (a bounded root solve).

## Mathematical Formulation

The forward area–Mach relation (inverted here for `M`):

$$ \frac{A}{A^*} = \frac{1}{M}\left[\left(\frac{2}{k+1}\right)\left(1 + \frac{k-1}{2}M^2\right)\right]^{(k+1)/[2(k-1)]} $$

> **Method:** bounded numerical inversion of the area–Mach relation on the branch selected by
> `regime$` (`M ≤ 1` subsonic, `M ≥ 1` supersonic).

## Examples

### Example 1 — Supersonic Mach at a nozzle shock station

A normal shock stands where `A/A* = 2.0`; the supersonic upstream Mach is
`M1 = mach_A_Astar(2.0, 1.4, 'supersonic')`.

[Run: cd-nozzle-shock]

**Expected:** `mach_A_Astar(2.0, 1.4, 'supersonic') ≈ 2.20` (the subsonic root is ≈ 0.31).

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `A_Astar` | Number | Yes | Area ratio A/A* (≥ 1, dimensionless). |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |
| `regime$` | String | Yes | Branch: `'subsonic'` or `'supersonic'`. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `M` | Number | Mach number on the selected branch. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `A_Astar < 1` | The area ratio cannot be below 1 (the sonic throat). Check the geometry. |
| `UNKNOWN_REGIME` | `regime$` not recognized | Use `'subsonic'` or `'supersonic'`. |
