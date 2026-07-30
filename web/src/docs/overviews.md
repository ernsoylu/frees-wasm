[Topic: lang-overview]
The frees language is small and declarative: equations are constraints, names are case-insensitive, and everything is computed in SI. These pages cover the grammar and the everyday building blocks — variables and the guesses that make nonlinear solves converge, units, arrays, complex numbers, strings, and the differentiable math functions. Start with *Equation Syntax & Rules* if you are new; the function pages list the most-used calls and link to the per-symbol Reference for full signatures.

[Topic: matrix-overview]
frees uses an array-language-style syntax for matrices and vectors. Declare a shape with a slice suffix, then add, multiply, transpose, or solve linear systems with the standard operators. For heavy numerics there are low-level OpenBLAS primitives and higher-level decompositions (LU, eigenvalues). Transfer-function coefficient arrays for control work are just vectors — see *Dynamic Systems & Control*.

[Topic: prog-overview]
When a model repeats or grows, factor it out. `FUNCTION` and `PROCEDURE` blocks add reusable, imperative-bodied routines; `MODULE` encapsulates a whole equation subsystem you can instantiate many times. `TABLE` blocks hold tabulated data callable like a function, and the lookup/interpolation and parametric-table accessors read that data back into a solve.

[Topic: fluids-overview]
frees ships high-precision property data so you never hand-look-up a state. CoolProp covers real fluids (water, refrigerants, ammonia, …); ideal-gas species use NASA polynomials; `AirH2O` handles humid air from three coordinates; and a built-in database carries bulk properties for common solids. Every property function returns SI base units. Group a circuit's state points with a `STATE TABLE` to isolate fluids and overlay cycles on property charts.

[Topic: solving-overview]
How frees actually solves — and what to do when it doesn't. These pages explain the pipeline (Tarjan blocking, then Newton's method per block), the guesses and bounds that make nonlinear systems converge, and a methodical debugging workflow for solves that stall. The same solved state powers two system-level analyses: first-order **uncertainty propagation** (`val ± unc` on every result) and **optimization** — parametric sweeps, single-objective search, and NSGA-II Pareto fronts.

[Topic: dynamics-overview]
Models that move. A `DYNAMIC` block integrates coupled, stiff, even event-driven ODE/DAE systems in time; `LINEARIZE` extracts state-space matrices about an operating point; and the control suite takes it from there — transfer functions, frequency response (Bode, Nyquist), pole placement, LQR, and PID tuning, with figures declared in code via `PLOT`. The symbolic CAS pages cover the Laplace-domain algebra that backs the control work.

[Topic: components-overview]
Model whole systems, not just equations: instantiate parameterized **components** (pumps, pipes, heat exchangers, resistors, gears, cooling coils, signal blocks — ~295 shipped), connect their ports, and frees expands the network into ordinary scalar equations for the same solver. The modeling is **acausal** (no inputs or outputs — fix any consistent boundary values), spans five physical domains plus a causal **signal** domain for command wires and six fluid families with strict cross-wiring guards, selects physics fidelity per component with `model$` variants, and turns the *same wiring* into a steady operating point or a transient. Start with *Your First Component Network*.

[Topic: deploy-overview]
frees is a client–server system you can run anywhere Docker runs. These pages explain the asynchronous compute model (API → queue → compute workers → job store) and why it makes solves robust and scalable, document the REST API so scripts can drive frees directly, and walk through both deployment paths: local Docker via `frees.sh`, and Railway (or any container platform) with the hard-won production configuration already baked in.

[Topic: tools-overview]
These are the tools around the editor that make modeling faster: a dockable **REPL** console that evaluates expressions against the last solved session (with the full `CALL` library and symbolic CAS), the **keyboard shortcuts** for Solve/Check, the **Markdown report** system that weaves live values and plots into a formatted document, and the **Graph Digitizer & Curve Fit** tools that turn a chart image or a table into a fitted equation.
