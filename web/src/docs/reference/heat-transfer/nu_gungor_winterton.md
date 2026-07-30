---
name: nu_gungor_winterton
category: Heat Transfer
summary: Nu, Gungor-Winterton flow boiling from liquid-only Nu. SIDE: boiling two-phase refrigerant. HX: evaporator refrigerant side
related: []
examples: []
tags: [nu, gungor, winterton, heat, transfer]
---

# nu_gungor_winterton

Nu, Gungor-Winterton flow boiling from liquid-only Nu. SIDE: boiling two-phase refrigerant. HX: evaporator refrigerant side


## Syntax

```
nu_gungor_winterton(Nu_l, Xtt, Bo)
```

## Description

Nu, Gungor-Winterton flow boiling from liquid-only Nu. SIDE: boiling two-phase refrigerant. HX: evaporator refrigerant side

## Mathematical Formulation

$$ Nu = Nu_l\big[1 + 3000\,Bo^{0.86} + 1.12(x/(1-x))^{0.75}(\rho_l/\rho_g)^{0.41}\big] \quad\text{(Gungor–Winterton)} $$

## Applicability

- **Where it applies:** Boiling two-phase refrigerant in evaporator tubes.
- **Valid when:** Saturated flow boiling, enhancing the liquid-only Nusselt with the boiling number and Martinelli parameter.
- **How it's used:** Evaporator refrigerant-side `h`; an alternative to the Chen (`chen_f`/`chen_s`) and Shah boiling models.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Nu_l` | Number | Yes | Liquid-only Nusselt number. |
| `Xtt` | Number | Yes | Turbulent–turbulent Martinelli parameter. |
| `Bo` | Number | Yes | Boiling number. |
