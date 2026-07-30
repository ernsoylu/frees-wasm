---
name: nu_traviss
category: Heat Transfer
summary: Nu, Traviss in-tube condensation. SIDE: condensing two-phase refrigerant. HX: tube/microchannel condenser refrigerant side
related: []
examples: []
tags: [nu, traviss, heat, transfer]
---

# nu_traviss

Nu, Traviss in-tube condensation. SIDE: condensing two-phase refrigerant. HX: tube/microchannel condenser refrigerant side


## Syntax

```
nu_traviss(Re_l, Pr_l, Xtt)
```

## Description

Nu, Traviss in-tube condensation. SIDE: condensing two-phase refrigerant. HX: tube/microchannel condenser refrigerant side

## Mathematical Formulation

$$ Nu = \frac{Pr_l\,Re_l^{0.9}\,F(X_{tt})}{F_2} \quad\text{(Traviss condensation)} $$

## Applicability

- **Where it applies:** Condensing two-phase refrigerant in tube/microchannel condensers.
- **Valid when:** In-tube condensation, annular-flow dominated.
- **How it's used:** Condenser refrigerant-side `h`; alternative to `nu_shah`/`nu_cavallini_zecchin`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Re_l` | Number | Yes | Liquid-only Reynolds number. |
| `Pr_l` | Number | Yes | Liquid Prandtl number. |
| `Xtt` | Number | Yes | Turbulent–turbulent Martinelli parameter. |
