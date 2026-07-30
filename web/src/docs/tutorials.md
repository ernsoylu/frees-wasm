[Topic: tut-msd]
# Tutorial: Mass–Spring–Damper, from Time Domain to Bode Plot

**The problem.** A 2 kg carriage on a spring (k = 800 N/m) with a viscous damper (c = 8 N·s/m) is released 5 cm from equilibrium. How does it ring down — and what does it look like as a plant, in the frequency domain?

**What you'll use:** `DYNAMIC` integration, ODE trajectory accessors, transfer functions, `CALL bode`, and `PLOT`. Build it in stages and solve after each one — that habit (from *Debugging a Solve*) pins any mistake to the lines you just added.

## Stage 1 — the parameters, and what to expect

```run
m = 2 [kg]
k = 800 [N/m]
c = 8 [N-s/m]

wn   = sqrt(k / m)              { natural frequency -> 20 rad/s }
zeta = c / (2 * sqrt(k * m))    { damping ratio     -> 0.1     }
```

Solve. With ζ = 0.1 the system is lightly underdamped: expect a slow ring-down at ~20 rad/s. Writing the analytic expectations *first* gives you numbers to check the simulation against — the habit that catches modeling errors.

## Stage 2 — free response in time

Newton's second law, as two first-order states:

```
DYNAMIC msd (method = ode45, time = 0 .. 5, points = 500)
  der(x) = v
  der(v) = -(c/m) * v - (k/m) * x
  energy = 0.5*m*v^2 + 0.5*k*x^2     { auxiliary output column }
  x(0) = 0.05
  v(0) = 0
END
```

Solve, open the **Tables** tab to see the ODE table, select `time` and `x`, and click **Plot curve**: a decaying oscillation. The `energy` column decays monotonically — a physical sanity check the plot gives you for free.

## Stage 3 — read the trajectory back

Trajectory accessors pull scalar answers out of the run, back into the analytic solve:

```
x_peak  = MaxValue('x')            { should be the 0.05 release }
t_cross = TimeAt('x', 0)           { first zero crossing }
E_final = FinalValue('energy')     { how much energy is left at t = 5 s }
```

Check `t_cross` against theory: the damped frequency is ωd = ωn·√(1−ζ²) ≈ 19.9 rad/s, so the first zero crossing lands near a quarter period, π/(2·ωd) ≈ 0.079 s.

## Stage 4 — the same plant in the frequency domain

Force-to-displacement, the plant is `X(s)/F(s) = 1/(m·s² + c·s + k)`. In frees a transfer function is just its coefficient vectors:

```
num = [1]
den = [m, c, k]
omega[1:400] = linspace(0.5, 100, 400)

CALL bode(num, den, omega : mag, phase)

PLOT 'MSD Bode'
  kind = bode
  omega = omega
  mag = mag
  phase = phase
END
```

Solve and open the **Plots** panel: the magnitude peaks at the resonance you predicted in Stage 1 (≈ 20 rad/s — for ζ = 0.1 the peak sits within a percent of ωn), and the phase falls through −90° there. One document now holds the physics, the transient, and the frequency response — all consistent because they share `m`, `c`, `k`.

## Pitfalls

- **Name your time axis `time`** if any state is named `t` or `T` — names are case-insensitive, and a collision between the time variable and a state is the classic first mistake (see *Transient / ODE Systems*).
- **No implicit multiplication:** `2*zeta*wn`, never `2 zeta wn`.
- If you stiffen the damper by orders of magnitude, switch `method = ode45` to `ode23s`.

## Complete listing

```run
{ Mass-spring-damper: ring-down, accessors, and Bode -- Tutorial 1 }
m = 2 [kg]
k = 800 [N/m]
c = 8 [N-s/m]
wn   = sqrt(k / m)
zeta = c / (2 * sqrt(k * m))

DYNAMIC msd (method = ode45, time = 0 .. 5, points = 500)
  der(x) = v
  der(v) = -(c/m) * v - (k/m) * x
  energy = 0.5*m*v^2 + 0.5*k*x^2
  x(0) = 0.05
  v(0) = 0
END

x_peak  = MaxValue('x')
t_cross = TimeAt('x', 0)
E_final = FinalValue('energy')

num = [1]
den = [m, c, k]
omega[1:400] = linspace(0.5, 100, 400)
CALL bode(num, den, omega : mag, phase)

PLOT 'MSD Bode'
  kind = bode
  omega = omega
  mag = mag
  phase = phase
END
```

## Go further

- Step and impulse responses: `CALL step(num, den, t : y)` on a time vector, plotted with the `xy` kind.
- Build the same oscillator from mechanical components (`TransMass`, a spring, a damper) and extract its state space with `LINEARIZE` — see *From Plant to Controller*.
- Close the loop: pick gains with `pidtune` or `lqr` and verify with `pole` and `margin`.

[Related: dynamic-ode, symbolic-cas, comp-linearize]

[Topic: tut-coil]
# Tutorial: Psychrometric Analysis of an AC Cooling Coil

**The problem.** An air handler draws 1.5 kg/s of dry air at 30 °C, 50 % relative humidity, and must deliver it at 12 °C, saturated. How much cooling does the coil need, how much of it is latent, and how much water condenses out?

**What you'll use:** the `AirH2O` psychrometric functions, energy and moisture balances — and then the **moist-air component library**, to build the same coil in four lines and see why the component layer exists. Solve after every stage.

## Stage 1 — the inlet state

Humid-air properties need **three** coordinates, one of which is total pressure (see *Psychrometrics*):

```
P_atm  = 101325 [Pa]
T_in   = 30 [C]
phi_in = 0.50
mdot   = 1.5 [kg/s]          { dry-air basis }

w_in  = HumRat(AirH2O, T=T_in, P=P_atm, R=phi_in)
h_in  = Enthalpy(AirH2O, T=T_in, P=P_atm, R=phi_in)
T_dew = DewPoint(AirH2O, T=T_in, P=P_atm, R=phi_in)
```

Solve: ω ≈ 0.0133 kg/kg, and the dew point lands near 18 °C. The coil surface will be far below that — so this coil dehumidifies, and the moisture balance in Stage 2 is not optional.

## Stage 2 — outlet state and coil duty

The air leaves at 12 °C saturated (relative humidity 1). Balances are on the **dry-air** mass basis, which is why psychrometric enthalpies are per kg of *dry air*:

```
T_out = 12 [C]
w_out = HumRat(AirH2O, T=T_out, P=P_atm, R=1)
h_out = Enthalpy(AirH2O, T=T_out, P=P_atm, R=1)

Q_total  = mdot * (h_in - h_out)              { total coil duty, W }
mdot_w   = mdot * (w_in - w_out)              { condensate, kg/s }
Q_latent = mdot * 2.501e6 * (w_in - w_out)    { latent share, W }
Q_sens   = Q_total - Q_latent
SHR      = Q_sens / Q_total                   { sensible heat ratio }
```

Solve. Expect a total duty around 45 kW with roughly 17 kW of it latent (SHR ≈ 0.6) and about 25 g/s of condensate — typical numbers for a deeply dehumidifying coil.

## Stage 3 — the same coil, as components

Now rebuild it from the moist-air library:

```
MoistAirSource AHU(P=P_atm, T=303.15 [K], W=w_in, mdot=1.5 [kg/s])
CoolingCoil    COIL(Tout=285.15 [K])
MoistAirSink   RET()

connect(AHU.out, COIL.in)
connect(COIL.out, RET.in)

Q_coil     = COIL.Q          { total duty, from the component }
Q_coil_lat = COIL.Q_lat      { latent share }
```

Solve: `COIL.Q` and `COIL.Q_lat` reproduce your Stage-2 numbers. The component carries the same physics you just wrote — saturated outlet, dry-air-basis balances — plus the connector bookkeeping: its ports conserve dry air (Σṁ_da = 0) and carry the humidity ratio `W` as a conserved rider (see *Domains & Fluid Families*).

The payoff is what happens next. Hand-written balances grow quadratically as the network grows; components don't:

## Stage 4 — grow it

- **Mixing box:** feed the coil 80 % return air at 26 °C / 55 % RH and 20 % outdoor air at 35 °C / 60 % RH through a `MixingBox` — it flow-weights both enthalpy *and* moisture at the junction, the step where hand calculations start sprouting errors.
- **Winter mode:** swap in `HeatingCoil` and `Humidifier` for the heating season.
- **Reheat:** add a `HeatingCoil` after the cooling coil to hit a supply setpoint at the dehumidified moisture level, and read both duties as named outputs.

## Complete listing

```run
{ AC cooling coil: psychrometrics by hand, then as components -- Tutorial 2 }
P_atm  = 101325 [Pa]
T_in   = 30 [C]
phi_in = 0.50
mdot   = 1.5 [kg/s]

w_in  = HumRat(AirH2O, T=T_in, P=P_atm, R=phi_in)
h_in  = Enthalpy(AirH2O, T=T_in, P=P_atm, R=phi_in)
T_dew = DewPoint(AirH2O, T=T_in, P=P_atm, R=phi_in)

T_out = 12 [C]
w_out = HumRat(AirH2O, T=T_out, P=P_atm, R=1)
h_out = Enthalpy(AirH2O, T=T_out, P=P_atm, R=1)

Q_total  = mdot * (h_in - h_out)
mdot_w   = mdot * (w_in - w_out)
Q_latent = mdot * 2.501e6 * (w_in - w_out)
Q_sens   = Q_total - Q_latent
SHR      = Q_sens / Q_total

{ The same coil from the moist-air library }
MoistAirSource AHU(P=P_atm, T=303.15 [K], W=w_in, mdot=1.5 [kg/s])
CoolingCoil    COIL(Tout=285.15 [K])
MoistAirSink   RET()
connect(AHU.out, COIL.in)
connect(COIL.out, RET.in)

Q_coil     = COIL.Q
Q_coil_lat = COIL.Q_lat
```

## Pitfalls

- **Every `AirH2O` call needs three coordinates including `P`** — two won't resolve, and inside a query `T` means *dry-bulb* (wet-bulb is the `B` coordinate).
- **Component temperatures are SI:** parameters like `Tout=285.15 [K]` — annotate a Celsius input on a plain variable and pass the variable if you prefer to think in °C.
- **Don't equate moist-air enthalpy across a coil that condenses** — water leaves the stream; that is exactly what the `W` bookkeeping is for.

[Related: humidair, comp-domains, comp-first-network]

[Topic: tut-rlc]
# Tutorial: Frequency Response of an RLC Filter

**The problem.** A series RLC circuit (R = 220 Ω, L = 0.1 H, C = 1 µF) driven by a 5 V source, with the output taken across the capacitor, is a second-order low-pass filter. What does it pass, what does it reject, and how peaked is it?

**What you'll use:** phasor (impedance) analysis with plain algebra, then the transfer-function route with `CALL bode` — the same circuit two ways, so you can check one against the other.

## Stage 1 — the numbers that shape the response

```run
R = 220 [ohm]
L = 0.1 [H]
C = 1e-6 [F]

w0 = 1 / sqrt(L * C)         { resonance -> 3162 rad/s (~503 Hz) }
Q  = w0 * L / R              { quality factor -> ~1.44 }
```

Solve. A `Q` above 1/√2 means the magnitude response will show a resonant peak just below ω₀ before rolling off at −40 dB/decade — worth knowing *before* you plot.

## Stage 2 — phasor analysis at one frequency

At a single frequency, the circuit is an impedance divider. The reactances and the divider work out with ordinary real algebra:

```
f  = 200 [Hz]
w  = 2 * pi# * f
X_L = w * L                       { inductive reactance }
X_C = 1 / (w * C)                 { capacitive reactance }
Z_mag = sqrt(R^2 + (X_L - X_C)^2) { series impedance magnitude }
phi   = arctan((X_L - X_C) / R)   { impedance angle, rad }

V_s   = 5 [V]
I_mag = V_s / Z_mag
V_out = I_mag * X_C               { amplitude across the capacitor }
gain  = V_out / V_s
```

At 200 Hz (well below resonance) the gain should come out near 1 — the filter passes it. Re-solve with `f = 2000 [Hz]` and the gain collapses; that is the roll-off, one point at a time. (frees also has native complex variables — the `_r`/`_i` pair mechanism in *Complex Numbers* — but for a single divider, real algebra is the shortest path.)

## Stage 3 — the whole response at once

Point-by-point is how you check; the transfer function is how you *see*. For the output across `C`:

$$ H(s) = \frac{1}{L C\,s^2 + R C\,s + 1} $$

```
num = [1]
den = [L * C, R * C, 1]
omega[1:400] = linspace(100, 30000, 400)
CALL bode(num, den, omega : mag, phase)

PLOT 'RLC Low-Pass Bode'
  kind = bode
  omega = omega
  mag = mag
  phase = phase
END
```

Solve and open the **Plots** panel: flat passband, the modest `Q` peak near 3162 rad/s, then −40 dB/decade. Read the Stage-2 spot check against the curve — they must agree, because they are the same physics.

## Complete listing

```run
{ Series RLC low-pass filter -- Tutorial 3 }
R = 220 [ohm]
L = 0.1 [H]
C = 1e-6 [F]

w0 = 1 / sqrt(L * C)
Q  = w0 * L / R

f  = 200 [Hz]
w  = 2 * pi# * f
X_L = w * L
X_C = 1 / (w * C)
Z_mag = sqrt(R^2 + (X_L - X_C)^2)
phi   = arctan((X_L - X_C) / R)
V_s   = 5 [V]
I_mag = V_s / Z_mag
V_out = I_mag * X_C
gain  = V_out / V_s

num = [1]
den = [L * C, R * C, 1]
omega[1:400] = linspace(100, 30000, 400)
CALL bode(num, den, omega : mag, phase)

PLOT 'RLC Low-Pass Bode'
  kind = bode
  omega = omega
  mag = mag
  phase = phase
END
```

## Pitfalls

- **Radians vs hertz:** `bode` and the plot axis work in ω (rad/s); convert with `w = 2 * pi# * f` and don't mix the two.
- **No implicit multiplication:** `2 * pi# * f`, never `2 pi# f`.

## Go further

- Take the output across `R` instead — the numerator becomes `[R*C, 0]` and the same circuit is a band-pass.
- Build the circuit from electrical components (`VoltageSource`, `Resistor`, `Capacitor`, `Inductor`, `Ground`) and watch the transient charging response in a `DYNAMIC` block.
- Check margins and poles with `margin` and `pole` as the start of a control loop around the filter.

[Related: complex, symbolic-cas, plot-code]

[Topic: tut-vccycle]
# Tutorial: A Refrigeration Cycle with Real Uncertainty

**The problem.** An R134a vapor-compression cycle evaporates at −10 °C and condenses at 40 °C with a 72 % isentropic compressor. What is the COP — and how well do you actually *know* that COP, given that the two temperatures come from ±0.5 K probes and the efficiency from a ±0.03 datasheet figure?

**What you'll use:** CoolProp property calls at the four cycle states, and the **uncertainty propagation engine** (`UncertaintyOf`) that turns instrument specs into error bars on the result — the calculation every test report needs and almost nobody does by hand.

## Stage 1 — the four states

Work around the loop, one state per pair of lines. Saturated states are identified by quality `x` plus one of `T` or `P` — never both (see *Debugging a Solve*):

```run
T_evap = 263.15 [K]     { -10 C }
T_cond = 313.15 [K]     {  40 C }
eta_c  = 0.72

{ 1: saturated vapor leaving the evaporator }
P1 = P_sat(R134a, T=T_evap)
h1 = Enthalpy(R134a, T=T_evap, x=1)
s1 = Entropy(R134a, T=T_evap, x=1)

{ 2: compressor discharge at condenser pressure }
P2  = P_sat(R134a, T=T_cond)
h2s = Enthalpy(R134a, P=P2, s=s1)        { isentropic ideal }
h2  = h1 + (h2s - h1) / eta_c            { real discharge enthalpy }

{ 3: saturated liquid off the condenser; 4: after the expansion valve }
h3 = Enthalpy(R134a, P=P2, x=0)
h4 = h3                                  { throttling: isenthalpic }
```

Solve. Check the pressure ratio `P2/P1` (should be near 5) — a sanity anchor before performance numbers.

## Stage 2 — performance

```
q_evap = h1 - h4          { refrigerating effect, J/kg }
w_comp = h2 - h1          { specific compressor work, J/kg }
COP    = q_evap / w_comp
```

Expect a COP a little above 3 for these conditions.

## Stage 3 — how well do you know it?

Attach the instrument specs directly in code. `UncertaintyOf(X) = value` declares the measurement uncertainty of an input; frees then propagates all of them through the whole system (finite-difference Jacobian, root-sum-square) and every computed variable in the Solution panel gains a `± band`:

```
UncertaintyOf(T_evap) = 0.5
UncertaintyOf(T_cond) = 0.5
UncertaintyOf(eta_c)  = 0.03
```

Solve again and read `COP` — now shown as `value ± uncertainty`. The dominant contributor is the efficiency figure, not the probes: a conclusion you get for free here, and the reason to buy a better datasheet before better thermometers.

## Complete listing

Drag the sliders to move the cycle's boundary temperatures and watch the COP re-solve — the falloff with condensing temperature is the whole story of air-conditioning on a hot day:

```run vary=T_evap=253.15:1:273.15 vary=T_cond=303.15:1:323.15
{ R134a vapor-compression cycle with uncertainty -- Tutorial 4 }
T_evap = 263.15 [K]     { -10 C }
T_cond = 313.15 [K]     {  40 C }
eta_c  = 0.72

P1 = P_sat(R134a, T=T_evap)
h1 = Enthalpy(R134a, T=T_evap, x=1)
s1 = Entropy(R134a, T=T_evap, x=1)

P2  = P_sat(R134a, T=T_cond)
h2s = Enthalpy(R134a, P=P2, s=s1)
h2  = h1 + (h2s - h1) / eta_c

h3 = Enthalpy(R134a, P=P2, x=0)
h4 = h3

q_evap = h1 - h4
w_comp = h2 - h1
COP    = q_evap / w_comp
PR     = P2 / P1

UncertaintyOf(T_evap) = 0.5
UncertaintyOf(T_cond) = 0.5
UncertaintyOf(eta_c)  = 0.03
```

## Pitfalls

- **Inside the dome, `T` and `P` are not independent** — identify saturated states with `x` plus one of them, or the solve stalls on a singular system.
- **Enthalpies are absolute J/kg** in SI — differences (`h2 - h1`) are what carry meaning, not the raw values.
- `UncertaintyOf` values are in the variable's **SI unit** (kelvin for the temperatures here); a relative spec must be converted first.

## Go further

- Overlay the cycle on a P-h chart: group the states with a `STATE TABLE` and add a `PLOT` of kind `property` (see *Fluid State Tables*).
- Rebuild the loop from two-phase components (`TwoPhaseCompressor`, `TwoPhaseCondenserFloat`, `TwoPhaseEvaporatorUA`) and let the pressures float with the boundary conditions.
- Sweep `T_cond` with a `PARAMETRIC` table to see the COP fall as ambient rises.

[Related: thermo, uncertainty, state-tables]

[Topic: tut-pump]
# Tutorial: Pump Selection from a Manufacturer's Curve

**The problem.** A cooling loop needs water lifted 18 m through a piping run with known friction. The candidate pump's head curve exists only as a picture in a datasheet. Find the operating point, and the shaft power it implies.

**What you'll use:** the **Graph Digitizer** to turn the datasheet picture into numbers, a unit-annotated `TABLE` as the pump model, and an implicit solve for the intersection with the system curve — the everyday workflow of turning *vendor paper* into *engineering numbers*.

## Stage 1 — digitize the curve

Open the **Graph Digitizer** (left toolbar), load the datasheet image, calibrate the axes with two known points on each, and click along the head curve (see *Graph Digitizer & Curve Fit* for the full workflow). Export the points, then paste them into a `TABLE` block — column 1 is flow, the header annotates the units, and the table becomes a callable function:

```
TABLE pump_curve(flow [m^3/s]) [Pa]
  0.000   520000
  0.010   505000
  0.020   470000
  0.030   415000
  0.040   340000
  0.050   245000
  0.060   130000
END
```

Anything computed from the `pump_curve` table now carries pascals, because the header says so.

## Stage 2 — the system curve

The circuit resists flow with a static head plus friction that grows with the square of flow:

```
rho = 998 [kg/m^3]
g   = 9.81 [m/s^2]
H_static  = 18 [m]
dP_static = rho * g * H_static     { ~176 kPa of static lift }
K = 1.1e8                          { friction coefficient, Pa/(m^3/s)^2 }
```

(`K` comes from your piping model — the Darcy losses of *Your First Component Network*, or a measured pressure drop at a known flow.)

## Stage 3 — the operating point

The pump runs where the curves cross. In frees that is one declarative line — the equation *is* the intersection:

```
pump_curve(V_op) = dP_static + K * V_op^2
```

No rearranging, no iteration loop of your own: the solver finds `V_op` (expect ≈ 0.039 m³/s). From there the engineering answers are arithmetic:

```
dP_op   = dP_static + K * V_op^2
eta_pump = 0.68
P_hyd   = dP_op * V_op             { hydraulic power, W }
P_shaft = P_hyd / eta_pump         { what the motor must supply }
```

## Complete listing

```run
{ Pump selection from a digitized head curve -- Tutorial 5 }
TABLE pump_curve(flow [m^3/s]) [Pa]
  0.000   520000
  0.010   505000
  0.020   470000
  0.030   415000
  0.040   340000
  0.050   245000
  0.060   130000
END

rho = 998 [kg/m^3]
g   = 9.81 [m/s^2]
H_static  = 18 [m]
dP_static = rho * g * H_static
K = 1.1e8

pump_curve(V_op) = dP_static + K * V_op^2

dP_op    = dP_static + K * V_op^2
eta_pump = 0.68
P_hyd    = dP_op * V_op
P_shaft  = P_hyd / eta_pump
```

## Pitfalls

- **Digitize past the region you expect to operate in** — interpolation is honest, extrapolation off the end of a table is not.
- **If the intersection solve stalls**, set a guess for `V_op` near the middle of the table's flow range in Variable Info (`Ctrl + I`) — an intersection far from the default guess is the classic case for one.
- **Keep the table in SI-consistent columns** (or annotate the units, as here) so the system-curve pascals and the table pascals actually meet.

## Go further

- Fit a quadratic to the digitized points with the **Curve Fit** panel and compare the smooth fit against the raw table.
- Sweep `H_static` with a `PARAMETRIC` table to see the operating point walk down the curve.
- Wrap the loop in components — `Pump` and `Pipe` — and let the network compute `K` from geometry instead of assuming it.

[Related: digitizer-fit, tables-code, optimization]
