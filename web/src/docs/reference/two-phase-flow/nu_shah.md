---
name: nu_shah
category: Two-Phase Flow
summary: Shah condensation Nusselt number
related: []
examples: []
tags: [nu, shah, two, phase, flow]
---

# nu_shah

Shah condensation Nusselt number


## Syntax

```
nu_shah(Re_l, Pr_l, x, p_red)
```

## Description

Returns the **in-tube condensation Nusselt number** by the Shah correlation — an enhancement on the liquid-only Nusselt number that captures the thinning film and vapor shear.

## Mathematical Formulation

$$ Nu_{TP} = Nu_l\left(1 + \frac{3.8}{Z^{0.95}}\right), \quad Z = (1/x - 1)^{0.8}p_r^{0.4} \quad\text{(Shah)} $$

## Applicability

- **Where it applies:** The condensing two-phase refrigerant side of a condenser / gas-cooler.
- **Valid when:** In-tube condensation across a wide quality and reduced-pressure range; depends on the reduced pressure `p_red`.
- **How it's used:** Gives the condensing film coefficient (`h = Nu·k_l/D_h`) for the refrigerant side. A robust general-purpose alternative to `nu_cavallini_zecchin` / `nu_traviss`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `Re_l` | Number | Yes | Liquid-only Reynolds number. |
| `Pr_l` | Number | Yes | Liquid Prandtl number. |
| `x` | Number | Yes | Vapor quality (0–1). |
| `p_red` | Number | Yes | Reduced pressure P/Pcrit. |
