---
name: Frequency-Domain Stability Analysis
category: Cookbook
guide: true
summary: Analyze a plant end to end — poles/zeros, gain & phase margins, Bode, Nyquist, step.
examples: [control-analysis-report]
tags: [cookbook, control, bode, nyquist, margin, stability, frequency response]
related: [pole, zero, margin, bode, nyquist, step]
---

# Frequency-Domain Stability Analysis

**Goal:** take a transfer function and characterize it completely — pole/zero
locations, gain and phase margins, the Bode and Nyquist responses, and the
time-domain step response — in one document.

## What you'll build

From `G(s) = num/den`:

1. `pole`/`zero` — the s-plane map and stability check.
2. `margin` — gain margin, phase margin, and crossover frequencies.
3. `bode`/`nyquist` — the frequency response, plotted inline.
4. `step` — the unit step response and its transient metrics.

## Approach

Stability is read three ways that must agree: all poles in the left
half-plane; positive gain/phase margins; and a Nyquist locus that does not encircle
`−1 + j0`. The frequency response evaluates `G(jω)` along the imaginary axis:

$$ \text{mag} = 20\log_{10}|G(j\omega)|,\qquad \text{phase} = \angle G(j\omega) $$

Each figure is declared by a named `PLOT … END` block and appears in the Plots window after the solve.

## Worked example

[Run: control-analysis-report]

**What it tells you:** for the underdamped plant `G(s) = (s+2)/(s²+4s+25)` — poles at
`−2 ± 4.58j` (stable, `ω_n = 5`, `ζ ≈ 0.4`), a resonant Bode peak near 5 rad/s, a
Nyquist locus clear of `−1`, and a step response that overshoots and rings before
settling.
