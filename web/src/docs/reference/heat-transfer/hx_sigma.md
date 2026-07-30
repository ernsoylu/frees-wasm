---
name: hx_sigma
category: Heat Transfer
summary: GEOMETRY: free-flow (contraction) ratio sigma=Aflow/Afrontal. SIDE: compact HX air/gas face
related: []
examples: []
tags: [hx, sigma, heat, transfer]
---

# hx_sigma

GEOMETRY: free-flow (contraction) ratio sigma=Aflow/Afrontal. SIDE: compact HX air/gas face


## Syntax

```
hx_sigma(Aflow, Afrontal)
```

## Description

GEOMETRY: free-flow (contraction) ratio sigma=Aflow/Afrontal. SIDE: compact HX air/gas face

## Mathematical Formulation

$$ \sigma = \frac{A_{\text{flow}}}{A_{\text{frontal}}} \quad\text{(free-flow / contraction ratio)} $$

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Aflow` | Number | Yes | Free-flow (minimum) cross-sectional area [m²]. |
| `Afrontal` | Number | Yes | Frontal (face) area [m²]. |
