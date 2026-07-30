---
name: M2_shock
category: Compressible Flow
summary: Downstream Mach number across a normal shock M2(M1, k).
related: [P2_P1_shock, P02_P01_shock, mach_A_Astar]
examples: [cd-nozzle-shock]
tags: [compressible, normal shock, mach, supersonic, subsonic, nozzle]
---

# M2_shock

Returns the **Mach number downstream of a normal shock** given the supersonic
upstream Mach `M1` and specific-heat ratio `k`. A normal shock always decelerates
the flow to subsonic (`M2 < 1`).

## Syntax

```
M2 = M2_shock(M1, k)
```

## Description

Across a normal shock the flow jumps discontinuously from supersonic to subsonic
while conserving mass, momentum, and energy. `M2` depends only on `M1` and `k`.

## Mathematical Formulation

$$ M_2 = \sqrt{\frac{(k-1)\,M_1^2 + 2}{2k\,M_1^2 - (k-1)}} $$

> **Method:** direct evaluation; valid for `M1 ≥ 1` (a shock requires supersonic
> inflow). `M1 = 1` gives `M2 = 1` (vanishing shock).

## Examples

### Example 1 — Subsonic Mach after a nozzle shock

[Run: cd-nozzle-shock]

**Expected:** at `M1 ≈ 2.20`, `k = 1.4`, `M2_shock ≈ 0.55`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `M1` | Number | Yes | Upstream Mach number (≥ 1, dimensionless). |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `M2` | Number | Downstream (subsonic) Mach number. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `M1 < 1` | A normal shock requires supersonic inflow; check the upstream state. |
