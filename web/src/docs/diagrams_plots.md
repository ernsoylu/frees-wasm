[Topic: plot-code]
# Plots in Code (PLOT)

Declare figures directly in your code with a `PLOT ... END` block. Each block names a figure (quoted title) and sets `kind` plus the data attributes for that kind. The figure appears in the Plots panel and can be embedded in reports (see below).

## XY plot (solved arrays)
```
PLOT 'Speed vs Distance'
  kind = xy
  x = speed[1:N]
  y = distance[1:N]
  xlabel = 'Speed [m/s]'
  ylabel = 'Distance [m]'
END
```

## Thermodynamic property plot
Overlays state points from a `STATE TABLE` of the named fluid onto a T-s or P-h chart. Set `overlaystates` to draw the points and `connectstates` to connect them as a cycle path.
```
PLOT 'Boiler Cycle'
  kind = property
  fluid = Water
  diagram = 'T-s'
  overlaystates = true
  connectstates = true
END
```

## Control-system plot kinds
Feed these the arrays produced by `bode`, `nyquist`, `pole`/`zero` (see *Control Systems & Symbolic CAS*):
- **Bode** — stacked magnitude (dB) and phase (deg) vs. frequency:
```
PLOT 'Bode Diagram'
  kind = bode
  omega = omega
  mag = mag
  phase = phase
END
```
- **Nyquist** — real vs. imaginary, with the `-1 + j0` critical point marked:
```
PLOT 'Nyquist Diagram'
  kind = nyquist
  real = re
  imag = im
END
```
- **Pole-zero map** — s-plane scatter (poles `x`, zeros `o`):
```
PLOT 'Pole-Zero Map'
  kind = polezero
  pr = pr
  pi = pi
  zr = zr
  zi = zi
END
```

Time responses (`step`, `impulse`, `lsim`) reuse the standard **xy** kind with the time vector on `x`. The root-locus and Nichols kinds take the matrices/arrays returned by `rlocus` and `nichols`.

## Embedding plots in reports
Reference any code-defined plot in your narrative with a graph tag and it renders as a live, interactive chart in the **Formatted** view:
```
[Graph="Boiler Cycle"] Temperature–entropy diagram of the power cycle [/Graph]
```
The name inside the quotes must match a `PLOT` block's title.

[Related: reports, symbolic-cas]
