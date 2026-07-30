---
name: SI Engine Cycle (Wiebe Heat Release)
category: Cookbook
guide: true
summary: Build a single-zone spark-ignition engine cycle and integrate its cylinder-pressure trace.
examples: [engine-cycle-wiebe]
tags: [cookbook, engine, wiebe, heat release, indicator diagram, powertrain, dynamic]
related: [wiebe_rate, AdiabaticFlameTemp]
---

# SI Engine Cycle (Wiebe Heat Release)

**Goal:** model a single-zone spark-ignition engine over the compression–combustion–
expansion strokes and integrate the **cylinder-pressure trace** for the indicator
(p–V) diagram.

## What you'll build

A crank-angle first-law model integrated by a `DYNAMIC` block:

- Cylinder volume from slider-crank kinematics (compression ratio, displacement).
- Burned-mass-fraction rate from a Wiebe function (`wiebe_rate`).
- `dp/dθ` from the first law, integrated over crank angle.

## Approach

The Wiebe burn rate spreads the total heat release `Q_tot` over the burn duration:

$$ \frac{dQ}{d\theta} = Q_{tot}\,\frac{dx_b}{d\theta},\qquad x_b = 1 - \exp\!\left[-a\left(\tfrac{\theta-\theta_0}{\Delta\theta}\right)^{m+1}\right] $$

The single-zone energy balance then gives `dp/dθ` as a function of the changing volume
and the instantaneous heat release; the `DYNAMIC` integrator marches it over the crank
angle. Plot `p` vs `V` for the indicator diagram.

## Worked example

[Run: engine-cycle-wiebe]

**What it tells you:** the cylinder-pressure history and the closed p–V loop whose
area is the indicated work. The burn looks bell-shaped (peaking partway through the
duration); advancing or retarding `θ_soc` shifts the peak pressure and the work.
