---
name: nu_colburn
category: Heat Transfer
summary: Nu=j*Re*Pr^(1/3). SIDE: air/gas through a compact finned surface. HX: plate-fin/louvered-fin air side
related: []
examples: []
tags: [nu, colburn, heat, transfer]
---

# nu_colburn

Nu=j*Re*Pr^(1/3). SIDE: air/gas through a compact finned surface. HX: plate-fin/louvered-fin air side


## Syntax

```
nu_colburn(j, Re, Pr)
```

## Description

Nu=j*Re*Pr^(1/3). SIDE: air/gas through a compact finned surface. HX: plate-fin/louvered-fin air side

## Mathematical Formulation

$$ Nu = j\,Re\,Pr^{1/3} \quad\text{(Colburn } j\text{-factor)} $$

## Applicability

- **Where it applies:** Air/gas through a compact finned surface.
- **Valid when:** Compact plate-fin / louvered-fin cores, characterized by a Colburn `j`-factor.
- **How it's used:** Air-side `h = j·Re·Pr^{1/3}·k/D_h`; pair the friction with `f_fin`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `j` | Number | Yes | Colburn j-factor. |
| `Re` | Number | Yes | Reynolds number. |
| `Pr` | Number | Yes | Prandtl number. |
