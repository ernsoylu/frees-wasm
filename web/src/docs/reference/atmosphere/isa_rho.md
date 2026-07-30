---
name: isa_rho
category: Atmosphere
summary: ISA 1976 density [kg/m^3] at geopotential altitude [m]
related: []
examples: []
tags: [isa, rho, atmosphere]
references:
  - "U.S. Standard Atmosphere, 1976 (NOAA/NASA/USAF)"
---

# isa_rho

ISA 1976 density [kg/m^3] at geopotential altitude [m]


## Syntax

```
isa_rho(alt)
```

## Description

ISA 1976 density [kg/m^3] at geopotential altitude [m]

## Mathematical Formulation

$$ \rho(h) = \frac{P(h)\,M}{R\,T(h)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `alt` | Number | Yes | Geopotential altitude [m]. |

## References

1. U.S. Standard Atmosphere, 1976 (NOAA/NASA/USAF).
