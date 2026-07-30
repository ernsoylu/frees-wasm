---
name: nu_hilpert
category: Heat Transfer
summary: Nu, single-cylinder cross-flow. SIDE: air/gas over a single tube. HX: bare-tube / sparse-bank air side
related: []
examples: []
tags: [nu, hilpert, heat, transfer]
---

# nu_hilpert

Nu, single-cylinder cross-flow. SIDE: air/gas over a single tube. HX: bare-tube / sparse-bank air side


## Syntax

```
nu_hilpert(Re, Pr)
```

## Description

Nu, single-cylinder cross-flow. SIDE: air/gas over a single tube. HX: bare-tube / sparse-bank air side

## Mathematical Formulation

$$ Nu = C\,Re^{m}\,Pr^{1/3} \quad\text{(single cylinder, Hilpert)} $$

## Applicability

- **Where it applies:** Air/gas over a single cylinder or a sparse bank.
- **Valid when:** Cross-flow over an isolated tube; band-dependent `C, m`.
- **How it's used:** Air-side `h` for bare-tube / low-density-bank exchangers.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Re` | Number | Yes | Reynolds number. |
| `Pr` | Number | Yes | Prandtl number. |
