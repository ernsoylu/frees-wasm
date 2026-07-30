---
name: hx_area_direct
category: Heat Transfer
summary: Primary (bare tube-wall) surface area of a fin-and-tube heat exchanger.
related: [hx_area_indirect, hx_eta_surf, ua_hx]
examples: [ev-thermal-management]
tags: [heat exchanger, geometry, primary area, tube wall, fin-and-tube]
---

# hx_area_direct

Returns the **primary surface area** [m²] — the exposed bare tube-wall area — of a
fin-and-tube heat-exchanger air side, from the core width, tube count, tube height,
core depth, and fin thickness. With `hx_area_indirect` (the fin
area) it gives the area split used by `hx_eta_surf`.

## Syntax

```
A_primary = hx_area_direct(W, tubeCount, Htube, depth, t)
```

## Description

The primary area is the tube outer wall exposed to the air stream — i.e. the tube
surface minus the footprint occupied by the fins. It is the fully-effective part of
the extended surface (efficiency 1).

## Mathematical Formulation

The exposed tube-wall area over all tubes, net of the fin-occupied fraction:

$$ A_{\text{primary}} = f\big(W,\,\text{tubeCount},\,H_{\text{tube}},\,\text{depth},\,t\big) $$

(a fin-and-tube geometric construction).

> **Method:** direct geometric evaluation of the bare tube-wall area.

## Examples

### Example 1 — Primary area of an air-side core

[Run: ev-thermal-management]

**Expected:** the bare tube-wall area, the smaller (fully-effective) part of the
air-side `A_total = A_primary + A_fin`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `W` | Number | Yes | Core width [m]. |
| `tubeCount` | Number | Yes | Number of tubes. |
| `Htube` | Number | Yes | Tube height/spacing [m]. |
| `depth` | Number | Yes | Core depth [m]. |
| `t` | Number | Yes | Fin thickness [m]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `A_primary` | Number | Primary (tube-wall) area [m²]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | A geometry input ≤ 0 | All dimensions must be positive. |
