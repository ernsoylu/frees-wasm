---
name: mass_flux
category: Heat Transfer
summary: GEOMETRY/flow: mass flux G=mdot/Aflow [kg/m^2/s] (any side)
related: []
examples: []
tags: [mass, flux, heat, transfer]
references: []
---

# mass_flux

GEOMETRY/flow: mass flux G=mdot/Aflow [kg/m^2/s] (any side)


## Syntax

```
mass_flux(mdot, Aflow)
```

## Description

GEOMETRY/flow: mass flux G=mdot/Aflow [kg/m^2/s] (any side)

## Mathematical Formulation

$$ G = \frac{\dot m}{A_{\text{flow}}} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `mdot` | Number | Yes | Mass flow rate [kg/s]. |
| `Aflow` | Number | Yes | Free-flow (minimum) cross-sectional area [m²]. |
