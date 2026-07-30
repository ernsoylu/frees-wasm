---
name: htc_extair
category: Heat Transfer
summary: External air-side heat-transfer coefficient — Zukauskas tube-bank cross-flow.
related: [htc_1phase, ua_hx, hx_eta_surf]
examples: [ev-thermal-management]
tags: [heat transfer, air side, zukauskas, tube bank, cross-flow, nusselt, film coefficient]
---

# htc_extair

Returns the **external air/gas-side heat-transfer coefficient** `h` [W/m²·K] for
cross-flow over a finned-tube bank, using the **Žukauskas** correlation. Use it for
the air side of a radiator, condenser, or cabin evaporator.

## Syntax

```
h = htc_extair(fluid$, P, T, mdot, D, Aflow)
```

## Description

The air-side film is usually the controlling resistance of an automotive heat
exchanger. The Žukauskas correlation gives the bank-averaged Nusselt number from
the maximum-velocity Reynolds number and Prandtl number, with a wall-property
correction.

## Mathematical Formulation

With $Re_{d,\max} = \rho V_{\max} D/\mu$ at the minimum free-flow area,

$$ Nu_d = C\,Re_{d,\max}^{\,n}\,Pr^{0.36}\left(\frac{Pr}{Pr_w}\right)^{1/4} $$

where `C` and `n` depend on the bank arrangement (in-line/staggered) and Reynolds
band; then $h = Nu_d\,k/D$.

> **Method:** properties at `(P, T)` → `Re_{d,max}`, `Pr` → Žukauskas `Nu` →
> `h = Nu·k/D`.

## Examples

### Example 1 — Radiator/condenser air-side film

[Run: ev-thermal-management]

**Expected:** an air-side film coefficient of order tens–low-hundreds W/m²·K — the
weak side that sets the overall `UA`, which is why it is finned.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `fluid$` | String | Yes | Gas name (typically air). |
| `P` | Number | Yes | Pressure [Pa]. |
| `T` | Number | Yes | Temperature [K]. |
| `mdot` | Number | Yes | Mass flow rate [kg/s]. |
| `D` | Number | Yes | Tube outer diameter [m]. |
| `Aflow` | Number | Yes | Minimum free-flow area [m²]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `h` | Number | Air-side coefficient [W/m²·K]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `D` or `Aflow` ≤ 0 | Provide positive geometry. |
