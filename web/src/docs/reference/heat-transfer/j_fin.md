---
name: j_fin
category: Heat Transfer
summary: Colburn j for a compact fin surface (plain|wavy|louvered|offset). SIDE: air/gas finned side. HX: plate-fin/louvered/offset-strip radiator, condenser, CAC
related: []
examples: []
tags: [fin, heat, transfer]
---

# j_fin

Colburn j for a compact fin surface (plain|wavy|louvered|offset). SIDE: air/gas finned side. HX: plate-fin/louvered/offset-strip radiator, condenser, CAC


## Syntax

```
j_fin(surface$, Re)
```

## Description

Colburn j for a compact fin surface (plain|wavy|louvered|offset). SIDE: air/gas finned side. HX: plate-fin/louvered/offset-strip radiator, condenser, CAC

## Mathematical Formulation

$$ j = St\,Pr^{2/3} = C\,Re^{m} \quad\text{(Colburn } j \text{ for the fin surface)} $$

## Applicability

- **Where it applies:** The air/gas finned side of a compact surface.
- **Valid when:** Plain / wavy / louvered / offset-strip fin surfaces (`surface$`).
- **How it's used:** Air-side `h` via the Colburn `j`-factor; pair with `f_fin`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `surface$` | String | Yes | Selector — One of `plain`, `wavy`, `louvered`, `offset`. |
| `Re` | Number | Yes | Reynolds number. |
