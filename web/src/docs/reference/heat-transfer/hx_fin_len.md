---
name: hx_fin_len
category: Heat Transfer
summary: Developed fin length of a fin-and-tube heat-exchanger air side.
related: [hx_area_indirect, hx_eta_surf, fin_efficiency]
examples: [ev-thermal-management]
tags: [heat exchanger, geometry, fin length, fin-and-tube, air side]
---

# hx_fin_len

Returns the **developed fin length** [m] of the finned air side of a fin-and-tube
heat exchanger from the core depth, fin thickness, fin density, and tube height —
a geometry helper feeding the fin area and efficiency.

## Syntax

```
finLen = hx_fin_len(depth, t, finDensity, Htube)
```

## Description

The developed (unrolled) length of the fin material between adjacent tubes, derived
from the core depth, the fin spacing implied by `finDensity`, and the tube-to-tube
gap `Htube`. It is the conduction length `L` used in the fin parameter
`mL = sqrt(2h/(k·t))·finLen`.

## Mathematical Formulation

The developed fin length combines the core depth and the inter-tube span at the
given fin pitch (`1/finDensity`) and thickness `t`:

$$ \text{finLen} = f\big(\text{depth},\,t,\,\text{finDensity},\,H_{\text{tube}}\big) $$

(a fin-and-tube geometric construction).

> **Method:** direct geometric evaluation of the developed fin path between tubes.

## Examples

### Example 1 — Fin length for the air-side fin parameter

[Run: ev-thermal-management]

**Expected:** the developed length that, with the fin thickness and conductivity,
sets `mL` and hence `fin_efficiency(mL)`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `depth` | Number | Yes | Core depth in the airflow direction [m]. |
| `t` | Number | Yes | Fin thickness [m]. |
| `finDensity` | Number | Yes | Fins per unit length [1/m]. |
| `Htube` | Number | Yes | Tube-to-tube vertical gap [m]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `finLen` | Number | Developed fin length [m]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | A geometry input ≤ 0 | All dimensions must be positive. |
