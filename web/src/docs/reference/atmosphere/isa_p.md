---
name: isa_p
category: Atmosphere
summary: ISA 1976 pressure [Pa] at geopotential altitude [m]
related: []
examples: []
tags: [isa, atmosphere]
references:
  - "U.S. Standard Atmosphere, 1976 (NOAA/NASA/USAF)"
---

# isa_p

ISA 1976 pressure [Pa] at geopotential altitude [m]


## Syntax

```
isa_P(alt)
```

## Description

ISA 1976 pressure [Pa] at geopotential altitude [m]

## Mathematical Formulation

$$ P(h) = P_b\left(\frac{T_b}{T_b + L_b(h-h_b)}\right)^{g_0 M/(R L_b)} \quad (L_b \ne 0) $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `alt` | Number | Yes | Geopotential altitude [m]. |

## References

1. U.S. Standard Atmosphere, 1976 (NOAA/NASA/USAF).
