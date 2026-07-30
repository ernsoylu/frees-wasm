---
name: T0_T
category: Compressible Flow
summary: Isentropic stagnation-to-static temperature ratio T0/T(M, k).
related: [P0_P, mach_A_Astar, stagnationtemp]
examples: [cd-nozzle-shock]
tags: [compressible, isentropic, stagnation, temperature, mach, nozzle]
---

# T0_T

Returns the **isentropic stagnation-to-static temperature ratio** `T0/T` for an
ideal gas at Mach number `M` with specific-heat ratio `k`. Use it to convert
between reservoir (stagnation) and local (static) temperature in nozzle, diffuser,
and duct flow.

## Syntax

```
ratio = T0_T(M, k)
```

## Description

Bringing a compressible stream isentropically to rest raises its temperature by
the kinetic-energy term; the ratio depends only on `M` and `k`. The static
temperature follows from a known stagnation value as `T = T0 / T0_T(M, k)`.

## Mathematical Formulation

$$ \frac{T_0}{T} = 1 + \frac{k-1}{2}\,M^2 $$

> **Method:** direct evaluation; dimensionless `M` and `k`.

## Examples

### Example 1 — Static temperature upstream of a nozzle shock

In a C-D nozzle, the supersonic static temperature at the shock station follows
from the reservoir temperature: `T1 = T0 / T0_T(M1, k)`.

[Run: cd-nozzle-shock]

**Expected:** at `M1 ≈ 2.20`, `k = 1.4`, `T0_T ≈ 1.965`, so `T1 ≈ 254 K` from `T0 = 500 K`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `M` | Number | Yes | Mach number (≥ 0, dimensionless). |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `ratio` | Number | Stagnation-to-static temperature ratio T0/T (≥ 1). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `M` negative or `k ≤ 1` | Use a non-negative Mach and a physical `k > 1`. |
