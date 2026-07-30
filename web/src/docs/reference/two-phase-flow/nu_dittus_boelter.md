---
name: nu_dittus_boelter
category: Two-Phase Flow
summary: Dittus-Boelter single-phase Nusselt 0.023 Re^0.8 Pr^n
related: []
examples: []
tags: [nu, dittus, boelter, two, phase, flow]
---

# nu_dittus_boelter

Dittus-Boelter single-phase Nusselt 0.023 Re^0.8 Pr^n


## Syntax

```
nu_dittus_boelter(Re, Pr, n)
```

## Description

Returns the **single-phase turbulent Nusselt number** `Nu = 0.023 Re^0.8 Pr^n`. It is both a stand-alone single-phase film coefficient and the **liquid-only baseline** that flow-boiling and condensation correlations enhance.

## Mathematical Formulation

$$ Nu = 0.023\,Re^{0.8}\,Pr^{n} \quad (n = 0.4 \text{ heating},\ 0.3 \text{ cooling}) $$

## Applicability

- **Where it applies:** Fully-developed turbulent single-phase flow in a tube — or the liquid-only reference inside a two-phase correlation.
- **Valid when:** Smooth tube, `Re ≳ 10⁴`, `0.7 ≲ Pr ≲ 120`; `n = 0.4` when heating the fluid, `0.3` when cooling.
- **How it's used:** Feeds the convective term of the Chen flow-boiling model and the liquid-only term of `nu_shah`/`nu_cavallini_zecchin`. Use `nu_gnielinski` for better transitional accuracy.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Re` | Number | Yes | Reynolds number. |
| `Pr` | Number | Yes | Prandtl number. |
| `n` | Number | Yes | Order / number of terms. |
