# Feature inventory — complete port checklist

Every feature in `../frEES/backend/core` (134 files, 38,181 LOC), mapped to its source files and the phase that ports it. LOC figures are from the Java source and are the best available proxy for effort.

Phases are defined in [`../PLAN.md`](../PLAN.md) §5.

---

## 1. Language and parsing — Phase 1

| Feature | Source | LOC |
|---|---|---|
| Grammar: 55 rules covering program/topLevel/blocks/statements/expressions/units | `core/src/main/antlr/Frees.g4` | 632 |
| Parser driver, document assembly, `autoSizeCallOutputs` | `parser/EquationParser.java` | 3,042 |
| Parse-tree → AST visitor | `parser/AstBuilder.java` | 1,587 |
| Expression AST (lowercased identifiers = case-insensitivity) | `ast/Expr.java` | 127 |
| Equation (residual `lhs - rhs`) | `ast/Equation.java` | 16 |
| Statement / top-level dispatch | `ast/Statement.java`, `ast/ProcDef.java`, `ast/ProcStatement.java` | 146 |
| Built-in physical & mathematical constants (`#` convention) | `parser/ConstantsRegistry.java` | 64 |
| String variables (`$` suffix) | `parser/StringVariables.java` | 155 |
| Intrinsic name/arity registry — **275 names** | `parser/FunctionRegistry.java` | 344 |
| Expression evaluator — **226 dispatch arms** | `ast/Evaluator.java` | 2,053 |
| `GUESS` directives (guess + bounds) | `ast/GuessDirective.java` | 11 |
| Blocks: `FUNCTION` · `PROCEDURE` · `MODULE` · `TABLE` · `PARAMETRIC` · `PLOT` · `STATETABLE` · `DYNAMIC` · `COMPONENT` · `LINEARIZE` · `SYMBOLIC` | grammar + respective AST files | — |

## 2. Units — Phase 1

| Feature | Source | LOC |
|---|---|---|
| Engineering unit table + unit-expression parser | `units/UnitRegistry.java` | 588 |
| Dimensional verification, SI derivation, non-blocking warnings | `units/UnitChecker.java` | 798 |
| Quantity (SI factor + dimension exponents) | `units/Quantity.java` | 93 |

**Invariant:** all calculation is in SI; unit warnings never block a solve. `TABLE`/`FUNCTION` argument and output units ground lookups so downstream variables resolve.

## 3. Steady solver — Phase 2

| Feature | Source | LOC |
|---|---|---|
| Full solve pipeline orchestration | `core/EquationSystemSolver.java` | 2,441 |
| Newton's method, numerical Jacobian, step-halving | `core/NewtonSolver.java` | 832 |
| Tarjan SCC decomposition into sequential blocks | `core/Blocker.java` | 384 |
| Block record | `core/Block.java` | 12 |
| Per-variable solver info (guess/bounds/limits) | `core/VariableSpec.java` | 36 |
| Stop criteria & complex mode | `core/SolverSettings.java` | 29 |
| Structured solver errors | `core/SolverException.java` | 58 |
| Check-before-solve (DOF + matching, no solve) | `web/api/CheckController.java` + core support | — |

## 4. Math kernels and procedural features — Phase 4

| Feature | Source | LOC |
|---|---|---|
| Symbolic partial differentiation of `Expr` | `ast/Differentiator.java` | 536 |
| Equation-based `Integral(f, t, a, b[, step])` | `core/IntegralSolver.java` | 439 |
| Complex-number expansion into `_r`/`_i` parts | `parser/ComplexExpansion.java` | 618 |
| Dense matrix CALLs (`SolveLinear`, `Inverse`, `Dot`, …) | `core/LinearAlgebra.java` | 108 |
| Regression kernels | `core/Statistics.java` | 48 |
| Discrete-signal kernels | `core/SignalProcessing.java` | 62 |
| Regular-grid 2-D interpolation (`Interp2`) | `core/Interpolation2D.java` | 65 |
| Curve-table interpolation | `core/CurveInterpolator.java` | 195 |
| Imperative `FUNCTION`/`PROCEDURE` execution | `parser/ProcedureEvaluator.java` | 142 |
| AST → LaTeX (Formatted Equations window) | `parser/LatexConverter.java` | 271 |

## 5. Properties, fluids and materials — Phase 5

| Feature | Source | LOC |
|---|---|---|
| Real-fluid properties over `PropsSI` | `props/PropertyFunctions.java` | 537 |
| CoolProp binding (4 C functions + LRU caches) | `props/CoolProp.java` | 215 |
| Psychrometrics via `HAPropsSI` | `props/Psychrometrics.java` | 148 |
| Bicubic `(P,h)` tables with **analytic** derivatives | `props/PhPropertyTable.java` | 289 |
| Phase-split `(P,h)` tables | `props/SaturationSplitTable.java` | 283 |
| Lazy per-fluid table registry | `props/PhTableRegistry.java` | 122 |
| Cubic EOS (SRK / Peng–Robinson) | `props/CubicEos.java` | 350 |
| Ideal-gas properties by chemical formula | `props/IdealGas.java` | 274 |
| NASA-7 two-range polynomial thermochemistry | `props/NasaThermo.java` | 140 |
| Combustion + ideal-gas-mixture thermochemistry | `props/Thermochemistry.java` | 218 |
| Stoichiometry helpers | `props/Combustion.java` | 93 |
| Equilibrium products with dissociation | `props/Equilibrium.java` | 288 |
| Chemical-formula parsing + molar mass | `props/ChemicalFormula.java` | 100 |
| Standard atomic weights | `props/PeriodicTable.java` | 55 |
| Ideal-gas transport (μ, k) | `props/GasTransport.java` | 105 |
| Compressible-flow relations | `props/CompressibleFlow.java` | 340 |
| ε-NTU, LMTD | `props/HeatExchanger.java` | 215 |
| HX sizing correlations (UA, ΔP) | `props/HxCorrelations.java` | 416 |
| Convective-heat correlations | `props/ConvectiveHeat.java` | 120 |
| Two-phase (Lockhart–Martinelli) constitutive fns | `props/TwoPhase.java` | 169 |
| Pneumatic (ISO 6358) constitutive fns | `props/Pneumatics.java` | 71 |
| Hydraulic/duct flow resistance | `props/FlowResistance.java` | 77 |
| ISA / U.S. 1976 standard atmosphere | `props/Atmosphere.java` | 45 |
| Bulk solid properties | `props/SolidProperties.java` | 128 |
| Wiebe heat-release engine sub-model | `props/Engine.java` | 51 |
| 1-D transient conduction (Heisler) | `core/HeislerCharts.java` | 114 |
| Property-diagram data (dome, isolines) | `props/PropertyDiagrams.java` | 324 |
| Vector export | `props/VectorExport.java` | 60 |
| Property error type | `props/PropertyEvaluationException.java` | 13 |

## 6. Component system (acausal, multi-domain) — Phase 6

| Feature | Source | LOC |
|---|---|---|
| Expansion of `COMPONENT`/`connect` to scalar equations | `parser/ComponentExpander.java` | 1,656 |
| Standard library loader | `parser/ComponentLibrary.java` | 86 |
| Component template / instance / connect AST | `ast/ComponentDef.java`, `ComponentInst.java`, `ConnectDecl.java` | 109 |
| Variable-Explorer component metadata | `api/ComponentMetadata.java` | 147 |
| Cycle-path resolution for property-plot overlays | `api/CyclePathResolver.java` | 669 |

**The 295-component library ports as data** (`include_str!`), not code:

| File | Components | Domain |
|---|---|---|
| `twophase.frees` | 47 | Two-phase thermofluid, TXV, three-zone HX |
| `signal.frees` | 34 | Causal signal/control blocks, `SigTable`, `SigPID` |
| `electrical.frees` | 31 | Electrical, battery, PEMFC |
| `fluid.frees` | 31 | Thermofluid / cycle |
| `mechanical.frees` | 27 | Rotational + translational |
| `hydraulic.frees` | 23 | Oil hydraulics, relief valve |
| `liquid.frees` | 21 | Liquid-side components |
| `moistair.frees` | 19 | Humid-air HVAC |
| `powertrain.frees` | 19 | Mean-value engine, transmission, road load |
| `pneumatic.frees` | 18 | ISO 6358 pneumatics |
| `heat.frees` | 17 | Thermal |
| `ac.frees` | 7 | Air-conditioning |
| `control.frees` | 1 | Control |

**Semantics that must survive the port:** four domains with `(across, flow)` pairs and junction rules (fluid `P`/`ṁ`+`h`, heat `T`/`Q̇`, electrical `V`/`I`, mechanical `ω`/`τ` and `v`/`F`); strict single-domain `connect` nodes as a **hard `ParseException`**; `domain$` connector-type separation (`fluid`/`gas`/`oil`/`moistair`); the moist-air `(P, ṁ_da, h, W)` basis with `W` flow-weighted only at a `MixingBox`; the gas-mixture `.y` species rider; `model$` `VARIANT … REQUIRE` selection; union-find loop closure; component-named (never mangled) diagnostics.

## 7. Dynamics — Phase 7

| Feature | Source | LOC |
|---|---|---|
| `DYNAMIC` block orchestration | `core/ode/DynamicSolver.java` | 1,194 |
| Time loop, step guards, dense output | `core/ode/OdeIntegrator.java` | 404 |
| Explicit RK stepper + tableaux | `core/ode/RungeKuttaMethod.java`, `ButcherTableau.java` | 251 |
| `ode15s` stiff BDF | `core/ode/BdfMethod.java` | 99 |
| `ode23s` Rosenbrock (2,3) | `core/ode/RosenbrockMethod.java` | 95 |
| FD Jacobians + dense solves for stiff paths | `core/ode/OdeLinearAlgebra.java` | 80 |
| Events / root finding | `core/ode/OdeEvent.java`, `OdeScalarFn.java` | 58 |
| ODE Table accessors (live during integration) | `core/ode/OdeAccessors.java`, `DynamicAccessorContext.java` | 315 |
| Problem/result/method types | `OdeProblem`, `OdeResult`, `OdeRhs`, `OdeMethod`, `OdeTableResult` | 283 |
| Static analysis of a `DYNAMIC` system | `core/ode/DynamicAnalysis.java` | 113 |
| Implicit DAE assembly `F(t,y,y')=0` | `core/dae/DaeAssembly.java`, `DaeResidual.java` | 108 |
| IDA system-matrix FD assembly | `core/dae/DaeJacobian.java` | 146 |
| DAE event functions | `core/dae/DaeRootFn.java` | 16 |
| High-level DAE solver façade | `core/dae/IdaDaeSolver.java` | 398 |
| SUNDIALS IDA binding | `core/dae/SundialsIda.java` | 207 |
| CSC + KLU sparse steady Newton path | `core/dae/SparseSteadyKlu.java` | 151 |
| `LINEARIZE` → numeric FD `(A,B,C,D)` | `ast/LinearizeSystem.java` + solver | 23 |

## 8. Analysis and design — Phase 8

| Feature | Source | LOC |
|---|---|---|
| Min/max optimization (Calculate ▸ Min/Max) | `core/Optimizer.java` | 719 |
| Multi-objective NSGA-II / Pareto fronts | `core/MultiObjectiveOptimizer.java` | 477 |
| All-roots search beyond a single Newton root | `core/AllRootsSolver.java` | 386 |
| Levenberg–Marquardt curve fitting | `core/CurveFitter.java` | 288 |
| Parameter estimation against measured data | `api/ParameterFit.java` | 298 |
| Monte Carlo uncertainty | `api/MonteCarlo.java` | 153 |
| Parametric run tables + live accessors | `ast/ParametricTable.java`, `core/ParametricAccessorContext.java` | 173 |
| Plot definitions / state tables | `ast/PlotDef.java`, `ast/StateTableDef.java` | 37 |
| Uncertainty propagation (FD Jacobian + SVD + RSS, `UncertaintyOf`) | within `EquationSystemSolver` | — |
| REPL dimensional analysis | `api/ReplDimensions.java` | 128 |
| Solve DTOs and shared API support | `api/SolveDtos.java`, `api/SolverApiSupport.java` | 585 |

## 9. CAS and control systems — Phase 9

| Feature | Source | LOC |
|---|---|---|
| CAS entry point (13 Symja ops) | `cas/CasEngine.java` | 211 |
| frees `Expr` → Symja string | `cas/ExprToSymja.java` | 123 |
| Symja output → frees expression | `cas/SymjaOutputNormalizer.java` | 25 |
| Single-expression CAS parsing | `cas/CasExpressions.java` | 72 |
| Symbolic identity coefficient solving | `cas/CasIdentity.java` | 112 |
| Polynomial utilities | `cas/PolynomialHelpers.java` | 988 |
| Transfer-function construction | `cas/TransferFunction.java` | 151 |
| Symbolic state-space ↔ transfer function | `cas/StateSpace.java` | 194 |
| Step / impulse / arbitrary time responses | `cas/TimeResponse.java` | 158 |
| LQR (Riccati) + pole placement | `cas/ControllerDesign.java` | 912 |
| PID auto-tuning | `cas/PidTuner.java` | 345 |
| `series`/`feedback`/`ss`/`tf` CALL flattening | `parser/ControlSystemsFlattener.java` | 1,978 |
| Control-systems intrinsic evaluation | `ast/ControlSystemsEvaluator.java` | 1,140 |

## 10. Data analyzer and measurement — Phase 10

| Feature | Source | LOC |
|---|---|---|
| ASAM MDF4 parsing | `measurement/Mf4Parser.java` | 254 |
| Parser seam + fallback ladder | `measurement/MeasurementParser.java`, `FallbackMeasurementParser.java` | 90 |
| Per-sample calculated signals | `measurement/TimeSeriesEvaluator.java` | 374 |
| Output raster construction | `measurement/MergedRaster.java` | 97 |
| Envelope decimation (twin of `decimate.ts`) | `measurement/EnvelopeDecimator.java` | 94 |
| Channel/metadata/window DTOs | `ChannelData`, `ChannelWindowDto`, `MeasurementMetadata`, `SampledSeries` | 184 |
| Parse error type | `measurement/MeasurementParseException.java` | 17 |

## 11. Frontend — reused unchanged

No port work. Dockable multi-window shell (dockview), CodeMirror editor with frees syntax highlighting, Solution/Arrays/Plots windows, REPL terminal, Variable Explorer, Excalidraw whiteboard, Univer spreadsheet, schematic tab, Data Analyzer (uPlot), Digitizer, Help/Docs, Examples, curve-fit and component wizards, About dialog.

**Only the transport layer changes.** The complete set of call sites — every one of which becomes a worker RPC with an identical promise signature:

`src/api.ts` (18): `runCompute` · `check` · `solve` · `replEvaluate` · `replClear` · `optimize` · `optimizeMulti` · `curveFit` · `parameterFit` · `getFluids` · `getReference` · `getPropertyDiagram` · `getPsychrometricChart` · `exportVector` · `solveTable` · `runMonteCarlo` · `pidTune` · `extractPlant`

`src/analyzer/measurementApi.ts` (4): `uploadMeasurement` · `fetchChannelWindow` · `calcSignal` · `deleteMeasurement`

## 12. Not ported — deliberately deleted

`compute/ComputeTask.java`, `compute/JobState.java`, `compute/JobTicket.java` (the RabbitMQ/Redis job protocol) and all of `backend/web` except its request/response DTO shapes. In-browser there is no broker, no job store, and no `202 Accepted`.

---

## Coverage check

| Package | Files | LOC | Phase |
|---|---|---|---|
| `ast/` | 17 | 4,286 | 1, 4, 9 |
| `parser/` | 11 | 9,943 | 1, 4, 6, 9 |
| `units/` | 3 | 1,479 | 1 |
| `core/` (root) | 19 | 6,852 | 2, 4, 5, 8 |
| `core/ode/` | 17 | 2,892 | 7 |
| `core/dae/` | 7 | 1,026 | 7 |
| `props/` | 28 | 5,246 | 5 |
| `cas/` | 11 | 3,291 | 9 |
| `api/` | 7 | 1,980 | 2, 6, 8 |
| `measurement/` | 11 | 1,110 | 10 |
| `compute/` | 3 | 76 | ❌ deleted |
| **Total** | **134** | **38,181** | |

Every file in `backend/core` is assigned, and the per-package sums reconcile exactly with the module total.

(`HeislerCharts.java` lives in `core/` but is listed under §5 Properties because that is the phase that ports it — the feature tables group by capability, the table above groups by package.)
