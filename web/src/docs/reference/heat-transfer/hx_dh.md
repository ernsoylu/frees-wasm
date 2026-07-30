---
name: hx_dh
category: Heat Transfer
summary: GEOMETRY: hydraulic diameter D_h=4*Aflow*L/Atotal [m] of a compact HX core (any side)
related: []
examples: []
tags: [hx, dh, heat, transfer]
---

# hx_dh

GEOMETRY: hydraulic diameter D_h=4*Aflow*L/Atotal [m] of a compact HX core (any side)


## Syntax

```
hx_dh(Aflow, Atotal, L)
```

## Description

GEOMETRY: hydraulic diameter D_h=4*Aflow*L/Atotal [m] of a compact HX core (any side)

## Mathematical Formulation

$$ D_h = \frac{4\,A_{\text{flow}}\,L}{A_{\text{total}}} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Aflow` | Number | Yes | Free-flow (minimum) cross-sectional area [m²]. |
| `Atotal` | Number | Yes | Total convective surface area [m²]. |
| `L` | Number | Yes | Length [m]. |
