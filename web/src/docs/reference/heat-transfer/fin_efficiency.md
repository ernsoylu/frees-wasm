---
name: fin_efficiency
category: Heat Transfer
summary: Efficiency of a straight fin with an insulated tip.
related: [hx_effectiveness, hx_eta_surf]
examples: [ev-thermal-management]
tags: [fin, efficiency, extended surface, tanh, conduction]
---

# fin_efficiency

Returns the **efficiency of a straight fin** with an insulated tip from its
dimensionless parameter `mL` — the ratio of the heat the fin actually dissipates
to what it would dissipate if its entire surface were at the base temperature.
Use it when sizing extended surfaces (fin-and-tube and plate-fin cores), typically
combined with `hx_eta_surf` into an overall surface efficiency.

## Syntax

```
eta = fin_efficiency(mL)
```

## Description

A real fin's temperature falls along its length as it sheds heat, so it is less
effective than an isothermal fin. The single group `mL` captures the competition
between convection off the surface and conduction along the fin. The result drops
from 1 (short/high-conductivity fin) toward 0 (long/poorly-conducting fin).

## Mathematical Formulation

For a straight fin of length $L$ with an insulated (adiabatic) tip,

$$ \eta_f = \frac{\tanh(mL)}{mL} $$

where the fin parameter follows from the 1-D fin energy balance,

$$ m = \sqrt{\frac{h\,P}{k\,A_c}} $$

for convection coefficient $h$, fin perimeter $P$, thermal conductivity $k$, and
cross-sectional area $A_c$.

> **Method:** direct evaluation; as $mL \to 0$, $\eta_f \to 1$.

## Examples

### Example 1 — Air-side fin efficiency in an EV condenser/radiator

The thermal-management sizing computes a fin parameter `mL` for each air-side
core and feeds `fin_efficiency(mL)` into the overall surface efficiency
`hx_eta_surf(...)` before forming `UA = h·A·η`.

[Run: ev-thermal-management]

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `mL` | Number | Yes | Fin parameter–length product `m·L` (dimensionless, ≥ 0), with `m = sqrt(h·P/(k·Ac))`. |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `eta` | Number | Fin efficiency η ∈ (0, 1]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| `DOMAIN_ERROR` | `mL` negative | `m` and `L` are positive; check `h`, `P`, `k`, `Ac` in `m = sqrt(h·P/(k·Ac))`. |
