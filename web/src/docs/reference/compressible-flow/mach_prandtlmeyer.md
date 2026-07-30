---
name: mach_prandtlmeyer
category: Compressible Flow
summary: Mach from Prandtl-Meyer angle [rad]
related: []
examples: []
tags: [mach, prandtlmeyer, compressible, flow]
---

# mach_prandtlmeyer

Mach from Prandtl-Meyer angle [rad]


## Syntax

```
mach_PrandtlMeyer(nu, k)
```

## Description

Mach from Prandtl-Meyer angle [rad]

## Mathematical Formulation

$$ \text{solve } \nu(M) = \nu_{\text{target}} \text{ for } M \quad (M \ge 1) $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `nu` | Number | Yes | Prandtl–Meyer angle [rad]. |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |
