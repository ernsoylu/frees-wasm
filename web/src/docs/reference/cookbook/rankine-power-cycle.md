---
name: Rankine Steam Power Cycle
category: Cookbook
guide: true
summary: Model an ideal steam Rankine cycle and compute its thermal efficiency.
examples: [rankine-cycle]
tags: [cookbook, rankine, steam, power cycle, efficiency, thermodynamics]
related: [Enthalpy, Entropy, Quality, Pump, Boiler, Turbine, Condenser]
---

# Rankine Steam Power Cycle

**Goal:** model the ideal steam power cycle and read off its **thermal efficiency**
from real-water (CoolProp) properties.

## What you'll build

[Diagram: RankineCycle]

Water is carried around four state points:

1. **Pump** — isentropic compression of saturated liquid to the boiler pressure.
2. **Boiler** — heat addition `q_in` to superheated vapor.
3. **Turbine** — isentropic expansion to the condenser pressure, producing `w_turb`.
4. **Condenser** — heat rejection `q_out` back to saturated liquid.

## Approach

Fix the boiler and condenser pressures, then evaluate each enthalpy from the
fluid state. The ideal-cycle balances are:

$$ w_{turb} = h_3 - h_4,\quad w_{pump} = h_2 - h_1,\quad q_{in} = h_3 - h_2,\quad \eta_{th} = \frac{w_{turb} - w_{pump}}{q_{in}} $$

The isentropic turbine sets `s_4 = s_3` (use `Entropy` at state 3, then
`Enthalpy` at the condenser pressure with that entropy). Check the
turbine-exit `Quality` — too low risks blade erosion (reheat fixes it).

## Worked example

[Run: rankine-cycle]

**What it tells you:** the thermal efficiency and the turbine-exit quality. Raising
the boiler pressure/temperature or lowering the condenser pressure increases `η_th`;
a reheat stage keeps the exit quality acceptable.

## Build it from components

A connected plant chains `Pump` → `Boiler` → `Turbine`
→ `Condenser` on a single `fluid$` stream.
