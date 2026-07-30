[Topic: analyzer]
# Data Analyzer (Measurements)

The **Data Analyzer** brings recorded measurement data — test-bench logs, ECU/vehicle recordings, exported simulation traces — into frees for time-series exploration and root-cause analysis. Open one from the wave icon in the left rail; each analyzer is its own dock window, and you can have several side by side.

## Importing data
- **CSV / TSV** files parse **in your browser** (nothing is uploaded): delimiters are auto-detected, a bracketed unit row under the header (`[s],[m/s],[Nm]`) is honored, and empty cells become gaps. The time column is found by name (`time`, `t`, `timestamp`, …), by monotonicity, and by format (ISO-8601 timestamps, epoch seconds/milliseconds, relative seconds). If it's ambiguous — or the data is index-based — a dialog asks you to pick a column or enter a fixed sample interval; frees never guesses silently. Duplicate or out-of-order timestamps are a hard import error naming the offending rows.
- **ASAM MDF4 (`.mf4`)** files upload to the backend (200 MB cap), are indexed there, and stream back as decimated windows — the browser never holds the raw file. Uncompressed files are parsed in-process; DZ-compressed recordings (deflate/ZSTD/LZ4, the usual OEM format) are handled by the asammdf sidecar when the deployment includes it. Channels keep their recorded units, channel groups with different rasters are all listed, and linear conversions are applied.
- Columns whose values are all 0/1 (or `true`/`false`) are tagged **bool** and drawn as stepped traces; text-valued channels are listed but not plottable.

## Oscilloscope
Signals are plotted in stacked **strips** that share one time axis. Add a strip, then click **+** next to a channel in the signal browser to assign it to the selected strip; colors are assigned from a fixed palette and stay stable across sessions.

- **Zoom**: drag a box, or scroll the mouse wheel centered on the pointer. **Double-click** (or *Reset zoom*) restores the full recording.
- Wide views draw a **min/max envelope**, so a single-sample spike or a one-sample boolean pulse is never lost, no matter how far you zoom out — the `envelope` badge shows when a strip is decimated.
- **Cursors**: click places cursor **A**, Shift+click places **B**; the readout shows t_A, t_B, Δt and 1/Δt. *Snap to samples* toggles between exact-sample and continuous placement, and ←/→ (Shift+←/→ for B) step a cursor one sample at a time.

## Instruments
All instruments share the same time range and cursors:

| Instrument | What it shows |
|---|---|
| **Table** | Every assigned signal by timestamp over the visible window, step-hold filled |
| **Statistics** | min / max / mean / median / std-dev per signal, plus v(A), v(B) and Δv — bound to the A–B range when both cursors are placed |
| **Events** | Rising-edge timestamps of a condition (boolean signal, or a threshold compare); clicking an event moves cursor A there and recenters the scope |
| **Scatter** | Signal-vs-signal correlation over the cursor-bounded range |
| **Histogram** | Value distribution of one signal over the same range |

## Multi-file compare & time offsets
Attach several files to one analyzer and mix their channels freely in strips. To synchronize recordings, give a file a **time offset**: type a precise Δt next to the file in the signal browser, or **Shift-drag** a strip to slide its first signal's file along the time axis. Offsets apply everywhere — strips, tables, statistics, events, and export.

## Saving and export
- **Export CSV** writes the assigned signals over the visible window on a merged raster (step-hold filled) — the file re-imports cleanly.
- The `.frees` project stores the analyzer's **layout and signal assignments, never the samples** (they can be gigabytes). Reopening a project shows the full layout with a *Locate file…* banner per file; one re-pick repopulates every strip. A wrong file — one missing the channels the analyzer uses — is rejected outright; a same-name file with a different size or content gets an explicit "use anyway" prompt.
- Server-side `.mf4` files are held per node with a time-to-live; if the backend restarts you'll see the same *Locate file…* banner and can simply re-upload.

[Related: calc-signals, plot-code, digitizer-fit]

[Topic: calc-signals]
# Calculated Signals

Calculated signals derive **new channels from measured ones using the frees expression language** — the same functions, operators and units that power the solver, applied per sample. That includes real-fluid property functions: turn a measured temperature/pressure pair into an enthalpy trace with one formula.

Open **Calc signal** in an analyzer's toolbar, write a formula, and bind each formula variable to a signal:

```
p_kw = tq * w / 1000
h_evap = enthalpy('R134a', T=t_suction, P=p_suction)
overspeed = speed > 25 AND gear >= 4
```

The result lands in the signal browser as a first-class channel (`ƒ name`) and is assigned to the selected strip — plot it, cursor it, export it, or feed it to the Event List like any recorded signal. A top-level condition (third example) produces a 0/1 boolean channel, which is exactly what the Event List consumes for complex triggers.

## Time operators
Four operators work on an input signal's history rather than a single sample:

| Operator | Meaning |
|---|---|
| `delta(x)` | sample-to-sample difference on the output raster |
| `integral(x)` | cumulative trapezoidal integral |
| `movavg(x, w)` | trailing mean over a `w`-second window |
| `delay(x, tau)` | the signal's value `tau` seconds earlier |

## Inputs, interpolation, raster
Each input has an **interpolation mode**: `linear` (default for continuous analog signals — step-holding a temperature into a nonlinear property function manufactures artificial spikes) or `step` (default for boolean/enum/ECU states). The **output raster** is the merged union of the input rasters, a fixed `dt`, or one input's own raster.

Rasters are capped (1M points, 100k when the formula calls functions). Exceeding the cap is a guided path, not a dead end: the error offers a one-click *switch to fixed dt* that fits. Heavy property-function jobs run on the compute tier automatically — the modal simply waits for the result.

[Related: analyzer, thermo, plot-code]
