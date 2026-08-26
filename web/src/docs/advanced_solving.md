[Topic: calculus]
# Numerical Integration (ODEs & Calculus)

`Integral(expr, var, lower, upper)` integrates `expr` with respect to `var` from `lower` to `upper`. Use it both for plain definite integrals and — via the self-reference trick below — for scalar first-order ODEs.

## Definite integration
```
{ Integrates 3*x^2 from 0 to 1 -> 1.0 }
area = Integral(3 * x^2, x, 0, 1)
```

## The ODE feedback pattern (scalar, first-order)
When `expr` contains the result variable itself, frees detects the self-reference and integrates the corresponding initial-value ODE **starting from 0 at the lower limit**. Because the integral always starts at 0, you integrate the *change* and rebuild the quantity of interest:

```run
{ Tank draining: dV/dt = -C*sqrt(V), V(0) = V0 }
V0 = 1.0
C = 0.02
{ integrate the DROP from 0..60 s, then rebuild V }
drop = Integral(-C * sqrt(V0 - drop), t, 0, 60)
V   = V0 - drop          { water volume at t = 60 s }
```

> **When to use this vs. `DYNAMIC`:** `Integral()` handles a single first-order ODE. For coupled, multi-state, stiff, or event-driven systems, use the `DYNAMIC` block on the next page instead.

[Related: dynamic-ode, optimization, variables]

[Topic: dynamic-ode]
# Transient / ODE Systems (DYNAMIC)

The `DYNAMIC ... END` block integrates coupled, multi-state ODE systems. A variable becomes a **state** the moment `der(X)` appears; each state needs one derivative equation and one initial condition. Algebraic auxiliaries (any equation without a `der`) are recomputed every step and become extra columns you can plot.

```
DYNAMIC name (method = solver, t = t0 .. tf, points = n_samples)
  der(state) = rate_equation
  state(0)   = initial_value
  auxiliary  = algebraic_calc      { an extra output column }
  EVENT name: condition -> stop | record
  EVENT name: g1 = g2 | rising -> set state = expr
END
```

An `EVENT` watches the zero crossing of its condition (`| rising` / `| falling` filters the direction). **`stop`** ends the run at the crossing; **`record`** logs it and continues; **`set state = expr`** reassigns a state at the crossing and restarts integration from the modified value — a *discrete* switch. That is what makes true hysteresis possible: a thermostat latch (`der(q) = 0`, one event setting `q = 1` at the low threshold, another setting `q = 0` at the high one) shows two distinct switching temperatures, which no smooth relay can. Give set events an explicit direction so the reassignment can't immediately retrigger itself.

[Diagram: GuessConvergence]

## Choosing a solver
| Need | Method | Notes |
| --- | --- | --- |
| General, non-stiff | `ode45` (default) | Dormand–Prince 5(4) adaptive. Start here. |
| Mildly stiff / cheaper | `ode23` | Bogacki–Shampine 3(2) adaptive. |
| Fixed-step teaching | `ode1`–`ode5` | Euler, Heun, RK3, RK4, Dormand–Prince. Use many points. |
| Stiff | `ode23s` / `ode15s` | Implicit Rosenbrock / BDF. Use when `ode45` needs tiny steps or stalls. |

Stiffness shows up when rates differ by orders of magnitude (e.g. fast chemistry alongside slow dynamics). If `ode45` runs slowly or the trajectory looks jagged, switch to `ode15s`.

## Trajectory accessors
Query columns of the compiled ODE Table from your analytic equations:
- **`FinalValue('col')`** — last value of column `col`.
- **`MaxValue('col')` / `MinValue('col')`** — peak / minimum.
- **`TimeAt('col', val)`** — time when `col` crosses `val`.
- **`ODEValue('col', t)`** — value interpolated at time `t`.

These let an ODE result feed back into the analytic solve — e.g. close a sizing loop with `MaxValue('h') = h_target`.

## Coupled two-state example
```run
m = 1.0; k = 20.0; c = 0.5
DYNAMIC mass_spring (method = ode45, t = 0 .. 20, points = 400)
  der(x) = v
  der(v) = -(c/m) * v - (k/m) * x
  energy = 0.5*m*v^2 + 0.5*k*x^2      { auxiliary column, decays }
  x(0) = 1.0
  v(0) = 0.0
END

final_displacement = FinalValue('x')   { read back into the analytic solve }
```

> **Name clash tip:** the time variable and a state named `T` are case-insensitively the same. Name the block's time axis `time` (or rename the state) to avoid the collision.

[Related: calculus, plot-code, symbolic-cas]

[Topic: optimization]
# Optimization & Parametric Sweeps

## Parametric sweeps
A `PARAMETRIC` block drives one or more variables across a range. Variables listed in the header with a range are **driven** (overridden each run); the rest are **computed** outputs. Open the **Tables** tab and click **Solve Table** (not the main Solve) to fill it in.

```
PARAMETRIC sweep_name(var1, var2, ...)
  var1 = start : step : end | Linear
END
```

### Sweep example
```
v0 = 50
g = 9.81
range_m = v0^2 * sin(2 * theta_deg * pi# / 180) / g

PARAMETRIC trajectory(theta_deg, range_m)
  theta_deg = 15 : 5 : 75 | Linear
END
```
Use the `| Log` suffix instead of `| Linear` for logarithmic spacing (handy for Bode frequency sweeps). Whole-table aggregates like `TableAvg('range_m')` or `IntegralValue('P','t')` are computed once and are identical in every row.

## Single-objective optimization
**Tools → Minimize / Maximize** (or the sidebar optimization panel) finds the value of one decision variable that minimizes or maximizes an objective, subject to your equation system. Set bounds on the decision variable in Variable Info (`Ctrl + I`) to keep the search in a physical region.

## Multi-objective optimization (Pareto front)
When objectives conflict (minimise mass *and* maximise efficiency, say) there is no single optimum — only a **Pareto front** of non-dominated trade-offs. frees traces it with **NSGA-II**: supply two or more objectives (each flagged minimise or maximise) plus the decision variables and their bounds. Each candidate solves the equation system with the decisions fixed; the result is a list of `(decisions, objectives)` points where no objective improves without worsening another. Plot one objective against the other to see the trade-off curve.

[Related: table-accessors, variables, plot-code]

[Topic: debugging]
# Debugging a Solve

When a model won't solve, the cause is almost always **structural** (the equation set) or a **guess** — not a bug in the solver. Work through it methodically instead of editing equations at random.

## Build incrementally
The fastest way to localize a failure is to *not* type the whole model and hit Solve. Enter a few equations, solve, then add the next group. This pins any new syntax or convergence error to the lines you just added.

- Press **F9** to *solve the selected block only*, ignoring the rest of the document — ideal for isolating one subsystem.
- Press **F4 (Check)** after each addition so the degrees of freedom stay balanced as you grow the model.

## Read the residuals and blocking order
frees groups the equations into strongly-connected **blocks** (Tarjan) and solves them in order. When a solve fails, the error message names *which block* stalled and its **residual**, and the **Diagnostics** section at the top of the Variable Explorer opens with the full picture: every equation's residual — the difference between its two sides — evaluated at the exact point the solver gave up, sorted by magnitude, with the failing block highlighted. The block that fails to converge, and the equation with the largest residual, is where to look first.

## Seed a guess for nonlinear blocks
Newton iterates from the guess in **Variable Info** (`Ctrl + I`); a guess near the expected magnitude is often the difference between converging and diverging. For a tightly coupled nonlinear block (radiation, simultaneous property inversions), bootstrap it with a **temporary equation**:

1. Temporarily replace the hard constraint with a rough explicit estimate, e.g. `T = (T_hot + T_cold) / 2`.
2. Solve — every variable now gets a physically sensible value.
3. Copy those values into the guesses (Variable Info), restore the real equation, and solve again from the good starting point.

## Common stalls
- **Singular Jacobian** — two equations are effectively the same, so the system is rank-deficient. A subtle version: two property calls from the *same* independent pair add no new information — `h = Enthalpy(Water, T=T, P=P)` together with `T = Temperature(Water, h=h, P=P)` just restate one relationship and stall the solver. Supply a genuinely independent equation instead.
- **Max iterations / diverged** — usually a guess or bound problem. Set a guess near the expected order of magnitude and bound the variable to its physical range (e.g. `T ≥ 0`, `0 ≤ x ≤ 1`).
- **DoF ≠ 0** — too few or too many equations; **F4** reports the imbalance before you solve. An accidental duplicate (the same equation written two ways) silently over-constrains the system.
- **Two-phase property lookups** — inside the vapor dome, temperature and pressure are *not* independent. Identify a saturated state by quality `x` with either `T` or `P`, never by `T` and `P` together.

[Related: variables, gs-units-check, api]

[Topic: errors]
# Errors & Diagnostics Index

Every message the checker and solver can raise, with its cause and fix. The red status pill in the app links here — find your message below. Two habits solve most of these before they happen: **F4 before F2**, and **build incrementally** (see *Debugging a Solve*).

## Syntax error on line N

The parser could not read that line. The usual culprits:

- **Implicit multiplication** — write `2 * x`, never `2x`.
- **A stray `==`** — frees has only the single `=` equality.
- **REPL-only syntax in the editor** — range literals like `[1 : 0.5 : 10]` and the symbolic CAS calls work only in the REPL; in a document build ranges with `x[1:n] = linspace(a, b, n)`.
- **Unbalanced brackets or block keywords** — every `FUNCTION`/`TABLE`/`DYNAMIC`/`COMPONENT`/`PLOT` needs its `END`.

## Degrees of freedom ≠ 0 (equations ≠ unknowns)

The system is under- or over-determined; the Check message reports both counts.

- **Too few equations** — a variable is mentioned but never constrained. Typos create this silently: `T_evap` vs `T_evp` are *two* variables.
- **Too many equations** — the same physics written twice. A classic: two property calls that restate one relationship (`h = Enthalpy(Water, T=T, P=P)` **and** `T = Temperature(Water, h=h, P=P)`), or re-equating pressures a component junction already equalizes.
- Case-insensitivity can *merge* names you meant to be distinct: `t` and `T` are the same variable.

## Singular Jacobian

Newton's step could not be computed — the equation block is rank-deficient at the current point.

- **Dependent equations**: two equations carry the same information (see the duplicate-physics cases above).
- **A guess on a flat spot**: the derivative vanishes at the guess (e.g. `x` starting exactly at a function's extremum). Nudge the guess in Variable Info (`Ctrl + I`).
- In component networks: a **re-equated mixer pressure** or a **loop with no pinned pressure level** — see *Troubleshooting Networks*.

## Newton iteration stalled / Max iterations

The solver ran but did not converge; the diagnostic names the failing **block** and its largest **residual**.

1. Set a **guess** near the expected magnitude for the block's variables (`Ctrl + I`), and **bounds** to keep the iteration physical (`T ≥ 0`, `0 ≤ x ≤ 1`).
2. If the block contains property calls, check the *property error* appended to the message — the iteration usually walked a state out of the fluid's valid range (next entry).
3. Bootstrap a stubborn block with a temporary explicit estimate, solve, copy values into guesses, restore the real equation (*Debugging a Solve* shows the pattern).

## Real-fluid range / property errors ("must be in range …")

A property was evaluated outside the fluid's validity envelope — almost always a symptom, not the disease: an unseeded unknown wandered there during iteration.

- Give the offending variable a physically sensible guess and bounds.
- **Inside the vapor dome, `T` and `P` are not independent** — identify a saturated state by quality `x` with `T` *or* `P`, never both; use `P_sat(fluid, T=…)` for the saturation line.
- For humid air, every `AirH2O` query needs **three** coordinates including total pressure `P`.

## Unit warnings ("dimensionally inconsistent …")

Warnings never block a solve, but they are usually pointing at a real slip: a missing `[unit]` annotation on an input, a formula mixing annotated and raw numbers, or a `TABLE`/`FUNCTION` without declared argument/output units. See *Units & Consistency*. Only trust results after the warning list is empty or understood.

## Component network errors

`connect` domain mismatches, port-count errors, mismatched fluid families, missing `PARAM`/`REQUIRE` values, and the shared-stream limit are all **deliberate hard errors** with their own page — see *Troubleshooting Networks*.

## Job FAILED without a diagnostic / stuck in PENDING

These come from the compute tier, not your model:

- **Stuck PENDING** — no compute workers are consuming the queue. Check `GET /api/health`: the `compute` entry counts live workers (see *Health & Scaling*).
- **FAILED after a worker crash** — a redelivered task is dropped by design (the poison-message guard) so one pathological job can't crash-loop the tier. Re-submit once; if it fails again, the model itself is killing the worker — reduce it and report.
- **429 Too many requests** — the API rate-limits bursts; wait a moment and retry.

[Related: debugging, comp-troubleshooting, api]

[Topic: api]
# Solver Reference & API

Knowing the execution pipeline helps you read convergence diagnostics and diagnose singular systems.

## Compilation & execution pipeline
1. **Lex/parse (ANTLR4)** — tokenizes variables, symbols, constants, and `[unit]` annotations.
2. **AST construction** — inlines functions/modules, expands array indices and matrix slices, and converts every unit to its SI base value.
3. **Dependency analysis (Tarjan SCC)** — builds the variable↔equation graph and groups coupled variables into minimal strongly-connected blocks.
4. **Newton–Raphson solve** — solves each block in topological order using finite-difference Jacobians and backtracking line search. Guesses (Variable Info) seed the iteration; bounds keep it physical.
5. **DYNAMIC pass** — integrates ODE blocks using the solved analytic variables as parameters. Accessor values feed back and the system re-solves until it converges globally.

## Reading convergence output
- **"Singular Jacobian"** — two equations are effectively dependent, or a guess landed on a flat region. Check for duplicate/redundant equations and adjust guesses.
- **"Max iterations"** — the solver didn't converge. Almost always a guess or bound problem; try a guess closer to the expected magnitude.
- **DoF ≠ 0** — too few or too many equations. F4 (Check) reports the imbalance before you solve.

[Related: variables, gs-units-check, uncertainty]
