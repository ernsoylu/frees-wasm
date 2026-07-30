---
name: isa_t
category: Atmosphere
summary: ISA 1976 temperature [K] at geopotential altitude [m]
related: []
examples: []
tags: [isa, atmosphere]
references:
  - "U.S. Standard Atmosphere, 1976 (NOAA/NASA/USAF)"
---

# isa_t

ISA 1976 temperature [K] at geopotential altitude [m]


## Syntax

```
isa_T(alt)
```

## Description

ISA 1976 temperature [K] at geopotential altitude [m]

## Mathematical Formulation

$$ T(h) = T_b + L_b\,(h - h_b) \quad\text{(layer lapse rate } L_b) $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `alt` | Number | Yes | Geopotential altitude [m]. |

## References

1. U.S. Standard Atmosphere, 1976 (NOAA/NASA/USAF).
