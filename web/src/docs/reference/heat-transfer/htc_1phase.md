---
name: htc_1phase
category: Heat Transfer
summary: Single-phase in-tube heat-transfer coefficient (Gnielinski / laminar).
related: [htc_evap, htc_cond, ua_hx]
examples: [ev-thermal-management]
tags: [heat transfer, convection, gnielinski, nusselt, single phase, tube, film coefficient]
---

# htc_1phase

Returns the **single-phase convective heat-transfer coefficient** `h` [W/m²·K] for
a fluid flowing in a tube/channel, from the fluid state and the flow geometry.
Turbulent flow uses the **Gnielinski** correlation; laminar flow falls back to the
constant-Nusselt limit. Use it for coolant/water/oil, refrigerant liquid lines, or
internal air — i.e. the single-phase side of a heat exchanger.

## Syntax

```
h = htc_1phase(fluid$, P, T, mdot, Dh, Aflow)
```

## Description

The function evaluates the fluid properties at `(P, T)`, forms the Reynolds and
Prandtl numbers from the mass flow and hydraulic diameter, applies the
turbulent/laminar Nusselt correlation, and returns `h = Nu·k/Dh`.

## Mathematical Formulation

With $Re = \dot m\,D_h/(A_{\text{flow}}\,\mu)$ and Darcy factor $f$, the Gnielinski
turbulent Nusselt number is

$$ Nu = \frac{(f/8)(Re-1000)\,Pr}{1 + 12.7\sqrt{f/8}\,\big(Pr^{2/3}-1\big)} $$

valid for $3000 \lesssim Re \lesssim 5\times10^6$; laminar flow uses the
fully-developed constant-Nu limit. Then

$$ h = \frac{Nu\,k}{D_h} $$

> **Method:** properties at `(P, T)` → `Re`, `Pr` → Gnielinski (turbulent) or
> constant-`Nu` (laminar) → `h = Nu·k/Dh`.

## Examples

### Example 1 — Coolant-side film in a chiller

[Run: ev-thermal-management]

**Expected:** a single-phase liquid film coefficient in the ~hundreds–few-thousand
W/m²·K range, well below the boiling/condensing refrigerant side.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `fluid$` | String | Yes | Fluid name. |
| `P` | Number | Yes | Pressure [Pa]. |
| `T` | Number | Yes | Temperature [K]. |
| `mdot` | Number | Yes | Mass flow rate [kg/s]. |
| `Dh` | Number | Yes | Hydraulic diameter [m]. |
| `Aflow` | Number | Yes | Free-flow cross-sectional area [m²]. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `h` | Number | Convective heat-transfer coefficient [W/m²·K]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `UNKNOWN_FLUID` | `fluid$` not resolvable | Use a supported fluid name. |
| `DOMAIN_ERROR` | `Dh` or `Aflow` ≤ 0 | Provide positive geometry. |
