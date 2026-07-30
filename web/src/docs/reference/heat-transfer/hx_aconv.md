---
name: hx_aconv
category: Heat Transfer
summary: Convective surface area of a compact heat-exchanger core from its geometry.
related: [hx_dh, hx_eta_surf, ua_hx]
examples: [ev-thermal-management]
tags: [heat exchanger, geometry, convective area, hydraulic diameter, compact core]
---

# hx_aconv

Returns the **convective (wetted) surface area** `A` [m²] of one side of a compact
heat-exchanger core from its free-flow area, flow length, and hydraulic diameter —
the area that enters `UA = h·A`.

## Syntax

```
A = hx_aconv(Aflow, L, Dh)
```

## Description

A compact core is characterized by its hydraulic diameter
`Dh = 4·Aflow·L/A_total`; inverting that definition gives the surface area for a
known free-flow area and flow length.

## Mathematical Formulation

$$ A = \frac{4\,A_{\text{flow}}\,L}{D_h} $$

the inverse of the hydraulic-diameter definition
$D_h = 4 A_{\text{flow}} L / A$.

> **Method:** direct evaluation from the compact-core geometry.

## Examples

### Example 1 — Core convective area for a UA estimate

[Run: ev-thermal-management]

**Expected:** the wetted area used with the side's film coefficient to build `UA`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Aflow` | Number | Yes | Free-flow (minimum) area [m²]. |
| `L` | Number | Yes | Flow length [m]. |
| `Dh` | Number | Yes | Hydraulic diameter [m]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `A` | Number | Convective surface area [m²]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `Dh ≤ 0` | Hydraulic diameter must be positive. |
