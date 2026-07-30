---
name: P02_P01_shock
category: Compressible Flow
summary: Stagnation pressure ratio across a normal shock P02/P01(M1, k).
related: [M2_shock, P2_P1_shock, P0_P]
examples: [cd-nozzle-shock]
tags: [compressible, normal shock, stagnation pressure, loss, irreversibility, nozzle]
---

# P02_P01_shock

Returns the **stagnation pressure ratio across a normal shock** `P02/P01` from the
upstream Mach `M1` and specific-heat ratio `k`. A shock is irreversible, so
stagnation pressure always drops (`P02/P01 < 1`) — this ratio quantifies the loss.

## Syntax

```
ratio = P02_P01_shock(M1, k)
```

## Description

Although static pressure rises across a shock, the entropy generated reduces the
stagnation (total) pressure. The recovered stagnation pressure downstream is
`P02 = P01 · P02_P01_shock(M1, k)`.

## Mathematical Formulation

$$ \frac{P_{02}}{P_{01}} = \left[\frac{(k+1)M_1^2}{2 + (k-1)M_1^2}\right]^{k/(k-1)}\left[\frac{k+1}{2k\,M_1^2 - (k-1)}\right]^{1/(k-1)} $$

> **Method:** direct evaluation; `≤ 1` with equality only at `M1 = 1`, decreasing
> as the shock strengthens.

## Examples

### Example 1 — Stagnation pressure loss across a nozzle shock

[Run: cd-nozzle-shock]

**Expected:** at `M1 ≈ 2.20`, `k = 1.4`, `P02_P01_shock ≈ 0.63`, so `P02 ≈ 628 kPa` from `P01 = 1 MPa`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `M1` | Number | Yes | Upstream Mach number (≥ 1, dimensionless). |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `ratio` | Number | Stagnation pressure ratio P02/P01 (≤ 1). |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `M1 < 1` | A normal shock requires supersonic inflow; check the upstream state. |
