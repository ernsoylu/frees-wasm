---
name: hx_eta_surf
category: Heat Transfer
summary: Overall surface (fin) efficiency of an extended-surface heat-exchanger side.
related: [fin_efficiency, ua_hx, htc_extair]
examples: [ev-thermal-management]
tags: [heat exchanger, fin, overall surface efficiency, extended surface, compact]
---

# hx_eta_surf

Returns the **overall surface efficiency** `η_o` of a finned heat-exchanger side —
the area-weighted blend of the fully-effective primary (bare) area and the
less-effective fin area. Multiply the film coefficient by `η_o` (or use
`η_o·h·A`) when forming `UA` for an extended surface.

## Syntax

```
eta_o = hx_eta_surf(Afin, Atotal, eta_fin)
```

## Description

On a finned surface, the primary tube wall sits at the base temperature
(efficiency 1) while the fins droop toward the fluid temperature (efficiency
`eta_fin` < 1, from `fin_efficiency`). The overall efficiency
weights the two by their area shares.

## Mathematical Formulation

$$ \eta_o = 1 - \frac{A_{\text{fin}}}{A_{\text{total}}}\big(1 - \eta_{\text{fin}}\big) $$

> **Method:** direct evaluation; `η_o = 1` for an unfinned surface
> (`A_fin = 0`), and `η_o → η_fin` as the fins dominate the area.

## Examples

### Example 1 — Air-side overall efficiency of a finned core

The EV thermal-management sizing forms `η_o` from the fin area share and
`fin_efficiency(mL)` before computing `UA = η_o·h·A`.

[Run: ev-thermal-management]

**Expected:** `η_o` between the bare value (1) and the fin efficiency — typically
0.7–0.95 for an automotive finned core.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Afin` | Number | Yes | Fin (secondary) surface area [m²]. |
| `Atotal` | Number | Yes | Total surface area (primary + fin) [m²]. |
| `eta_fin` | Number | Yes | Single-fin efficiency (from `fin_efficiency`). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `eta_o` | Number | Overall surface efficiency η_o ∈ (0, 1]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `Afin > Atotal` | The fin area cannot exceed the total area. |
