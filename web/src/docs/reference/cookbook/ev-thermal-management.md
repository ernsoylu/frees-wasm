---
name: EV Thermal-Management System
category: Cookbook
guide: true
summary: Couple a coolant loop, a refrigerant loop, and a cabin into one acausal system model.
examples: [ev-thermal-management]
tags: [cookbook, ev, thermal management, coolant, refrigerant, chiller, multi-domain, system]
related: [ua_hx, htc_1phase, htc_evap, htc_cond, dp_2phase, hx_eta_surf, LiquidPump, TwoPhaseCompressor, Chiller]
---

# EV Thermal-Management System

**Goal:** model a complete electric-vehicle thermal-management system — a coolant
loop and a refrigerant loop coupled through a chiller — as one acausal model whose
heat exchangers are **sized from correlations and geometry**, not hand-set `UA`.

## What you'll build

[Diagram: EvThermal]

Three interacting sub-systems solved together:

- **Coolant loop (EG50):** a pump feeds a branch split (battery + motor), rejoined and
  pushed through a radiator that rejects heat to ambient.
- **Refrigerant loop (R1234yf):** a compressor drives a condenser → expansion → evaporator
  circuit whose head pressure floats with ambient and load.
- **Cross-domain bridge:** the chiller's refrigerant evaporator wall is tied to the
  battery-branch coolant — heat crosses domains in one solve.

## Approach

Every exchanger's conductance is **built from first principles** rather than guessed:
each side's film coefficient comes from a correlation (`htc_1phase` for
single-phase coolant, `htc_evap`/`htc_cond` for the boiling/
condensing refrigerant, `htc_extair` for the air side), combined with the
wall through `ua_hx`; pressure drops follow from `dp_2phase` and
the compact-core geometry helpers. The fin sides use
`hx_eta_surf`·`fin_efficiency`.

The coupled network expands to scalar equations and solves through the standard
Newton/Tarjan pipeline — the heat balances close themselves (e.g. chiller `Q_ref ≡ Q_cool`).

## Worked example

[Run: ev-thermal-management]

**What it tells you:** the operating point of the whole system — coolant and
refrigerant flows, the floating condenser/evaporator pressures, the battery-chiller
duty, and every `UA` that the geometry implies. Change the ambient temperature or a
fan speed and the floating pressures and duties re-solve consistently.
