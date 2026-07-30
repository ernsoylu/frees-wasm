---
name: Rating a Heat Exchanger (ε-NTU)
category: Cookbook
guide: true
summary: Find a heat exchanger's duty and outlet temperatures from its UA by the effectiveness-NTU method.
examples: [hx-effectiveness-ntu]
tags: [cookbook, heat exchanger, effectiveness, ntu, rating, duty, heat transfer]
related: [hx_effectiveness, hx_NTU, LMTD, ua_hx]
---

# Rating a Heat Exchanger (ε-NTU)

**Goal:** given an exchanger's conductance `UA` and the two inlet streams, find the
**heat duty and both outlet temperatures** — without iterating on the unknown
outlets (which the LMTD method would require).

## What you'll build

A rating calculation in four steps:

1. Form each stream's heat-capacity rate `C = ṁ·cp`; take `Cmin`, `Cmax`.
2. Compute `NTU = UA/Cmin` and the capacity ratio `Cr = Cmin/Cmax`.
3. Get the effectiveness ε from the arrangement.
4. Back out the duty and outlets.

## Approach

The effectiveness–NTU relations give ε directly per flow arrangement
(`hx_effectiveness`), so the duty follows from the inlets alone:

$$ \varepsilon = f(NTU, C_r),\quad Q = \varepsilon\,C_{min}(T_{h,in} - T_{c,in}),\quad T_{h,out} = T_{h,in} - \frac{Q}{C_h},\ T_{c,out} = T_{c,in} + \frac{Q}{C_c} $$

Use `hx_NTU` for the inverse (sizing to a target ε) and `LMTD` to
cross-check the mean driving temperature.

## Worked example

[Run: hx-effectiveness-ntu]

**What it tells you:** the duty `Q ≈ 312 kW`, the outlet temperatures
(`Th_out ≈ 323 K`, `Tc_out ≈ 340 K`) and the effectiveness `ε ≈ 0.71` for the
counterflow case — read straight from `UA` with no iteration.
