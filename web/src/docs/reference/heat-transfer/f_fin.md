---
name: f_fin
category: Heat Transfer
summary: Fanning friction for a compact fin surface. SIDE: air/gas finned side dP (pair with j_fin). HX: same as j_fin
related: []
examples: []
tags: [fin, heat, transfer]
---

# f_fin

Fanning friction for a compact fin surface. SIDE: air/gas finned side dP (pair with j_fin). HX: same as j_fin


## Syntax

```
f_fin(surface$, Re)
```

## Description

Fanning friction for a compact fin surface. SIDE: air/gas finned side dP (pair with j_fin). HX: same as j_fin

## Mathematical Formulation

$$ f = C_f\,Re^{m_f} \quad\text{(Fanning friction for the fin surface)} $$

## Applicability

- **Where it applies:** The air/gas finned side of a compact surface.
- **Valid when:** Same fin surfaces as `j_fin`.
- **How it's used:** Air-side friction (Fanning) for the core `ΔP`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `surface$` | String | Yes | Selector — One of `plain`, `wavy`, `louvered`, `offset`. |
| `Re` | Number | Yes | Reynolds number. |
