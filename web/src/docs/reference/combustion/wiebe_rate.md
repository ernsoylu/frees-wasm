---
name: wiebe_rate
category: Combustion
summary: Wiebe heat-release rate dxb/dθ for an engine combustion model.
related: [wiebe, AdiabaticFlameTemp]
examples: [engine-cycle-wiebe]
tags: [combustion, engine, wiebe, vibe, heat release, burn rate, crank angle]
---

# wiebe_rate

Returns the **Wiebe (Vibe) burn rate** `dxb/dθ` — the rate of change of burned mass
fraction with crank angle — for a single-zone engine heat-release model. Multiply
by the total heat release to get the instantaneous heat-release rate `dQ/dθ` that
drives the cylinder-pressure trace.

## Syntax

```
rate = wiebe_rate(theta, theta0, dtheta, a, m)
```

## Description

The Wiebe function is the standard empirical S-curve for the cumulative mass-fraction
burned over a combustion event; its derivative is the bell-shaped heat-release rate.
`theta0` is the start of combustion, `dtheta` the burn duration, `a` the efficiency
parameter (≈ 5 for ~99% completion), and `m` the form factor (≈ 2 for SI engines).

## Mathematical Formulation

Burned mass fraction and its rate:

$$ x_b(\theta) = 1 - \exp\!\left[-a\left(\frac{\theta-\theta_0}{\Delta\theta}\right)^{m+1}\right] $$

$$ \frac{dx_b}{d\theta} = \frac{a(m+1)}{\Delta\theta}\left(\frac{\theta-\theta_0}{\Delta\theta}\right)^{m}\exp\!\left[-a\left(\frac{\theta-\theta_0}{\Delta\theta}\right)^{m+1}\right] $$

> **Method:** direct evaluation; the rate is zero before `theta0` and decays to
> zero as the burn completes.

## Examples

### Example 1 — SI-engine heat release over crank angle

The single-zone cycle integrates `dQ/dθ = Q_tot · wiebe_rate(θ, θ_soc, θ_dur, 5, 2)`
to build the cylinder-pressure trace.

[Run: engine-cycle-wiebe]

**Expected:** a bell-shaped release peaking partway through the burn duration
(`a = 5`, `m = 2`), zero outside `[θ_soc, θ_soc + θ_dur]`.

## Input Arguments

| Argument | Type | Required | Description |
| --- | --- | --- | --- |
| `theta` | Number | Yes | Current crank angle [deg]. |
| `theta0` | Number | Yes | Start of combustion [deg]. |
| `dtheta` | Number | Yes | Burn duration [deg]. |
| `a` | Number | Yes | Efficiency parameter (≈ 5 for ~99% completion). |
| `m` | Number | Yes | Form factor (≈ 2 for SI engines). |

## Output Arguments

| Argument | Type | Description |
| --- | --- | --- |
| `rate` | Number | Burn rate dxb/dθ [1/deg]. |

## Common Errors

| Error | Cause | Fix |
| --- | --- | --- |
| (zero result) | `theta < theta0` | The rate is zero before combustion starts — expected. |
