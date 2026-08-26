---
name: Vapor-Compression Refrigeration Cycle
category: Cookbook
guide: true
summary: Build an R134a vapor-compression cycle and compute its COP from real-refrigerant properties.
examples: [refrigeration-vcr]
tags: [cookbook, refrigeration, vcr, cop, refrigerant, cycle, thermodynamics]
related: [Enthalpy, Quality, Compressor, Condenser, ExpansionValve, TwoPhaseEvaporator]
---

# Vapor-Compression Refrigeration Cycle

**Goal:** model the standard four-process refrigeration cycle and read off its
**coefficient of performance (COP)** using real-refrigerant properties.

## What you'll build

[Diagram: RefrigerationCycle]

The cycle walks one refrigerant (R134a here) around four state points:

1. **Evaporator** — saturated/superheated vapor leaves at the low pressure, absorbing `q_L`.
2. **Compressor** — isentropic (or efficiency-corrected) compression to the high pressure.
3. **Condenser** — heat rejection `q_H`, leaving saturated/subcooled liquid.
4. **Expansion valve** — isenthalpic throttle back to the low pressure.

## Approach

Anchor the two pressures by saturation temperatures, then evaluate each state's
enthalpy from a real-fluid property call. The cycle energy balances are:

$$ q_L = h_1 - h_4,\quad w_c = h_2 - h_1,\quad q_H = h_2 - h_3,\quad \text{COP} = \frac{q_L}{w_c} $$

with the isenthalpic valve giving `h_4 = h_3`. Use `T_sat`/`P_sat`
to set the pressures and `Enthalpy` (with quality or superheat) for each point.

## Worked example

[Run: refrigeration-vcr]

**What it tells you:** the COP — cooling delivered per unit compressor work — and
how it falls as the condensing/evaporating temperature spread widens. Swapping the
fluid name (e.g. to R1234yf) re-evaluates every property in place.

## Build it from components

For a *connected* circuit rather than a state-by-state script, assemble the
two-phase component library: `TwoPhaseCompressor` →
`TwoPhaseCondenser` → `TwoPhaseExpansionValve`
→ `TwoPhaseEvaporator`, closed into a loop (see the
*EV Thermal-Management System* guide for a full coupled example).
