---
name: chen_f
category: Two-Phase Flow
summary: Chen flow-boiling convective enhancement factor F
related: []
examples: []
tags: [chen, two, phase, flow]
---

# chen_f

Chen flow-boiling convective enhancement factor F


## Syntax

```
chen_f(X_tt)
```

## Description

Returns the **convective enhancement factor `F`** of the Chen flow-boiling model — the factor by which two-phase convection exceeds the liquid-only value, as a function of the Martinelli parameter.

## Mathematical Formulation

$$ F = \big[1 + X_{tt}^{-1}\big]^{0.736} \text{-type convective enhancement (Chen)} $$

## Applicability

- **Where it applies:** Saturated flow boiling of a refrigerant in evaporator tubes.
- **Valid when:** Saturated (not subcooled) flow boiling; `F ≥ 1`, rising as the vapor fraction grows.
- **How it's used:** Used with `chen_s` in the Chen superposition `h = F·h_conv + S·h_nb`, where `h_conv` is the liquid-only convective coefficient.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `X_tt` | Number | Yes | Turbulent–turbulent Martinelli parameter. |
