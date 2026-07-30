---
name: P0_P
category: Compressible Flow
summary: Isentropic stagnation-to-static pressure ratio P0/P(M, k).
related: [T0_T, mach_A_Astar, stagnationpres]
examples: [cd-nozzle-shock]
tags: [compressible, isentropic, stagnation, pressure, mach, nozzle]
---

# P0_P

Returns the **isentropic stagnation-to-static pressure ratio** `P0/P` for an ideal
gas at Mach number `M` with specific-heat ratio `k`. Use it to convert between
reservoir and local pressure in isentropic nozzle and diffuser flow.

## Syntax

```
ratio = P0_P(M, k)
```

## Description

For isentropic flow the pressure ratio is the temperature ratio raised to
`k/(k−1)`. The local static pressure follows from a known stagnation value as
`P = P0 / P0_P(M, k)`.

## Mathematical Formulation

$$ \frac{P_0}{P} = \left(1 + \frac{k-1}{2}\,M^2\right)^{\!k/(k-1)} $$

> **Method:** direct evaluation; consistent with `T0_T` via the isentropic
> relation `P0/P = (T0/T)^{k/(k-1)}`.

## Examples

### Example 1 — Static pressure upstream of a nozzle shock

The supersonic static pressure at the shock station: `P1 = P0 / P0_P(M1, k)`.

[Run: cd-nozzle-shock]

**Expected:** at `M1 ≈ 2.20`, `k = 1.4`, `P0_P ≈ 10.6`, so `P1 ≈ 94 kPa` from `P0 = 1 MPa`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `M` | Number | Yes | Mach number (≥ 0, dimensionless). |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `ratio` | Number | Stagnation-to-static pressure ratio P0/P (≥ 1). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `M` negative or `k ≤ 1` | Use a non-negative Mach and a physical `k > 1`. |
