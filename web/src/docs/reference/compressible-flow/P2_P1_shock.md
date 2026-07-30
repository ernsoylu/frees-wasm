---
name: P2_P1_shock
category: Compressible Flow
summary: Static pressure ratio across a normal shock P2/P1(M1, k).
related: [M2_shock, P02_P01_shock, P0_P]
examples: [cd-nozzle-shock]
tags: [compressible, normal shock, pressure, rankine-hugoniot, nozzle]
---

# P2_P1_shock

Returns the **static pressure ratio across a normal shock** `P2/P1` from the
upstream Mach `M1` and specific-heat ratio `k`. The flow compresses across the
shock, so `P2/P1 > 1`.

## Syntax

```
ratio = P2_P1_shock(M1, k)
```

## Description

The static pressure rises sharply across a normal shock. The downstream static
pressure follows from the upstream value as `P2 = P1 · P2_P1_shock(M1, k)`.

## Mathematical Formulation

$$ \frac{P_2}{P_1} = \frac{2k\,M_1^2 - (k-1)}{k+1} $$

> **Method:** direct evaluation (Rankine–Hugoniot static pressure ratio); valid
> for `M1 ≥ 1`.

## Examples

### Example 1 — Static pressure jump across a nozzle shock

[Run: cd-nozzle-shock]

**Expected:** at `M1 ≈ 2.20`, `k = 1.4`, `P2_P1_shock ≈ 5.47`, so `P2 ≈ 514 kPa` from `P1 ≈ 94 kPa`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `M1` | Number | Yes | Upstream Mach number (≥ 1, dimensionless). |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `ratio` | Number | Static pressure ratio P2/P1 (≥ 1). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `M1 < 1` | A normal shock requires supersonic inflow; check the upstream state. |
