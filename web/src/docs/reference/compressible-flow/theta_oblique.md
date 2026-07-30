---
name: theta_oblique
category: Compressible Flow
summary: Oblique-shock deflection from wave angle [rad]
related: []
examples: []
tags: [theta, oblique, compressible, flow]
---

# theta_oblique

Oblique-shock deflection from wave angle [rad]


## Syntax

```
theta_oblique(M1, beta, k)
```

## Description

Oblique-shock deflection from wave angle [rad]

## Mathematical Formulation

$$ \tan\theta = 2\cot\beta\,\frac{M_1^2\sin^2\beta - 1}{M_1^2(k + \cos 2\beta) + 2} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `M1` | Number | Yes | Upstream Mach number (≥ 1). |
| `beta` | Number | Yes | Oblique-shock wave angle [rad]. |
| `k` | Number | Yes | Ratio of specific heats (e.g. 1.4 for air). |
