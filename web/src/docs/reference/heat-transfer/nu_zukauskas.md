---
name: nu_zukauskas
category: Heat Transfer
summary: Nu, tube-bank cross-flow. SIDE: air/gas over a tube bank. HX: fin-and-tube radiator/condenser air side
related: []
examples: []
tags: [nu, zukauskas, heat, transfer]
---

# nu_zukauskas

Nu, tube-bank cross-flow. SIDE: air/gas over a tube bank. HX: fin-and-tube radiator/condenser air side


## Syntax

```
nu_zukauskas(Re, Pr)
```

## Description

Nu, tube-bank cross-flow. SIDE: air/gas over a tube bank. HX: fin-and-tube radiator/condenser air side

## Mathematical Formulation

$$ Nu = C\,Re_{\max}^{m}\,Pr^{0.36}\,(Pr/Pr_w)^{1/4} \quad\text{(tube bank)} $$

## Applicability

- **Where it applies:** Air/gas in cross-flow over a tube bank (the air side of a fin-and-tube radiator/condenser).
- **Valid when:** External cross-flow; the constants `C, m` depend on the in-line/staggered arrangement and the Reynolds band.
- **How it's used:** Gives the air-side film coefficient `h = Nu·k/D`; combine with the refrigerant/coolant side and wall via `ua_hx`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Re` | Number | Yes | Reynolds number. |
| `Pr` | Number | Yes | Prandtl number. |
