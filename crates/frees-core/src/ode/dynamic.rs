//! `DYNAMIC` block orchestration — the transient system.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/core/ode/DynamicSolver.java`
//! (1,194 LOC) together with `ast/DynamicSystem.java`, whose records this module
//! carries because the wasm parser has no home for them yet.
//!
//! # The block is an index-1 semi-explicit DAE
//!
//! Each state's derivative is reified as a fresh unknown `der$<state>`; every
//! `der(X)` reference in the body is rewritten to that unknown, the `der(X) = …`
//! equation becomes `der$X = …`, and the algebraic auxiliaries keep their
//! `name = …` form. At each step the states and time are pinned and this
//! combined algebraic block is solved (the shared per-step **inner solve**),
//! yielding both the auxiliaries and the `der$` values that are `dy`. All states
//! therefore advance on one shared step cursor — the multi-state capability the
//! single-state `Integral()` lacks.
//!
//! # A variable is a state iff `der(X)` appears for it
//!
//! Every state needs exactly one `der(X) = …` and exactly one `X(t0) = …`.
//! Array states `der(T[i])` reuse the `FOR` / array machinery: the loops are
//! expanded against the constants the analytic solve resolved, and `T[3]`
//! becomes the scalar variable `t[3]` — the same naming the analytic array
//! machinery uses. That is what makes a method-of-lines PDE work.
//!
//! # The block is routed out of the analytic equation stream
//!
//! The parser hands `DYNAMIC` bodies over as [`DynamicSystem`]s rather than
//! statements, so the steady solver never sees a `der()`. The coupling back is
//! [`crate::ode::accessors`]: the analytic system reads the solved table through
//! `ODEValue` / `FinalValue` / `MaxValue` / `TimeAt` / the column aggregates.
//!
//! # Two deliberate omissions from the Java
//!
//! * **`deadlineNanos`.** Every Java entry point threads a wall-clock budget
//!   through to the integrator. `wasm32-unknown-unknown` has no clock — exactly
//!   as [`crate::integral`] documents for `IntegralSolver` — so the parameter is
//!   absent and the driver's own step budget is the only bound.
//! * **The SUNDIALS IDA path.** [`is_ida_method`] still classifies the method
//!   names, and [`DynamicSolver::dae_parts`] still assembles the implicit
//!   `F(t, y, y') = 0` form (residual template, `id` vector, sparsity, seeded
//!   `y0`/`yp0`, root information) — that assembly is pure frees semantics and is
//!   ported here. What is *not* here is the native solver call: `dae/` owns the
//!   pure-Rust replacement, and until it is wired [`DynamicSolver::solve`]
//!   refuses an `ida` method with a diagnosable error instead of silently
//!   integrating it with an explicit method.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::ast::{Equation, Expr, Statement};
use crate::dae::assembly::{AssemblySpec, DaeAssembly, EventSpec};
use crate::diag::{FreesError, Result};
use crate::eval::{eval_with, EvalContext, Scope};
use crate::ode::events::{bind_events, DynamicEvent, EventBinding, OdeEvent, StateReset};
use crate::ode::problem::{OdeProblem, OdeResult, OdeTableResult, TableEventHit};
use crate::parser::defs::Definitions;

// ---------------------------------------------------------------------------
// The AST records (port of ast/DynamicSystem.java)
// ---------------------------------------------------------------------------

/// Solver configuration parsed from the header
/// `DYNAMIC name (method = ode45, t = t0 .. tf [s], points = …, …)`.
///
/// Port of `DynamicSystem.Options`. Numeric bounds are stored in SI. `points`
/// (sample count) and `step` (fixed step) are nullable — a `None` `step` means
/// adaptive.
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicOptions {
    /// Integration scheme (`ode45`, `ode23s`, `ode15s`, `ida`, …).
    pub method: String,
    /// The header's independent variable, e.g. `t` in `t = 0 .. 60`.
    pub time_var: String,
    /// Start time.
    pub t0: f64,
    /// End time.
    pub tf: f64,
    /// Output sample count; `None` uses [`DynamicOptions::DEFAULT_POINTS`].
    pub points: Option<usize>,
    /// Fixed step size; `None` means adaptive.
    pub step: Option<f64>,
    /// Relative tolerance.
    pub rtol: f64,
    /// Absolute tolerance.
    pub atol: f64,
    /// Cap on a single step; `None` lets [`DynamicSolver::solve`] apply the
    /// span/100 default.
    pub max_step: Option<f64>,
}

impl DynamicOptions {
    /// `DynamicSystem.Options.DEFAULT_METHOD`.
    pub const DEFAULT_METHOD: &'static str = "ode45";
    /// `DynamicSystem.Options.DEFAULT_RTOL`.
    pub const DEFAULT_RTOL: f64 = 1e-6;
    /// `DynamicSystem.Options.DEFAULT_ATOL`.
    pub const DEFAULT_ATOL: f64 = 1e-9;
    /// `DynamicSystem.Options.DEFAULT_POINTS`.
    pub const DEFAULT_POINTS: usize = 200;

    /// The header defaults, with `t` as the independent variable over `0 .. 1`.
    /// A parser that omits an option should fall back to these values.
    pub fn defaults(time_var: impl Into<String>, t0: f64, tf: f64) -> DynamicOptions {
        DynamicOptions {
            method: DynamicOptions::DEFAULT_METHOD.to_string(),
            time_var: time_var.into(),
            t0,
            tf,
            points: None,
            step: None,
            rtol: DynamicOptions::DEFAULT_RTOL,
            atol: DynamicOptions::DEFAULT_ATOL,
            max_step: None,
        }
    }
}

/// An initial condition `X(t0) = value`, or `X[idx](t0) = value` for an array
/// state.
///
/// Port of `DynamicSystem.InitialCondition`. `indices` is empty for a scalar
/// state; a single `Expr::Range` index expands over its whole range. The value
/// expression is already unit-converted to SI.
#[derive(Debug, Clone, PartialEq)]
pub struct InitialCondition {
    /// The state's base name, lowercase.
    pub state: String,
    /// Array subscripts, empty for a scalar state.
    pub indices: Vec<Expr>,
    /// The initial value expression.
    pub value: Expr,
}

/// A transient / ODE system declared by a `DYNAMIC … END` block.
///
/// Port of `ast/DynamicSystem.java`. Parallel to the parametric table and plot
/// definitions: it is routed out of the analytic equation stream by the parser
/// so the analytic solver never sees a `der()` operator.
///
/// The body is carried structurally and only fully classified at solve time
/// (after the analytic solve resolves constants like `N`): [`for_blocks`] are
/// method-of-lines loops expanded against those constants, then every body
/// equation whose left side is `der(X)` is a *state-derivative* equation and the
/// rest are algebraic auxiliaries (output columns).
///
/// [`for_blocks`]: DynamicSystem::for_blocks
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicSystem {
    /// Block name — the ODE table / graphing-surface name.
    pub name: String,
    /// Solver configuration from the header.
    pub options: DynamicOptions,
    /// Top-level `der(X) = …` and algebraic `aux = …` equations.
    pub body_equations: Vec<Equation>,
    /// Method-of-lines `FOR` loops, expanded at solve time. Every element is a
    /// [`Statement::For`]; anything else is ignored, exactly as the Java's
    /// `List<Statement.For>` type makes impossible.
    pub for_blocks: Vec<Statement>,
    /// Initial conditions `X(t0) = expr`.
    pub initials: Vec<InitialCondition>,
    /// Zero-crossing events.
    pub events: Vec<DynamicEvent>,
    /// Original block text, for diagnostics.
    pub source_text: String,
}

// ---------------------------------------------------------------------------
// The per-step inner solve
// ---------------------------------------------------------------------------

/// Solves the algebraic block of a `DYNAMIC` system with a set of variables
/// pinned to fixed values, seeded by `warm_start`; returns the full value map.
///
/// Port of `DynamicSolver.AlgebraicSolve`. The production implementation is the
/// engine's `solve_pinned` (the Java `EquationSystemSolver.solvePinned`), which
/// is also what the `Integral` quadrature drives — the dynamic path shares the
/// analytic solver's Newton/Tarjan machinery rather than owning a second one.
///
/// `pinned` is an ordered list rather than a map because the pins become
/// `var = value` equations and the caller controls their order.
pub trait AlgebraicSolve {
    fn solve(
        &mut self,
        ordinary: &[Equation],
        pinned: &[(String, f64)],
        warm_start: Option<&Scope>,
    ) -> Result<Scope>;
}

impl<F> AlgebraicSolve for F
where
    F: FnMut(&[Equation], &[(String, f64)], Option<&Scope>) -> Result<Scope>,
{
    fn solve(
        &mut self,
        ordinary: &[Equation],
        pinned: &[(String, f64)],
        warm_start: Option<&Scope>,
    ) -> Result<Scope> {
        self(ordinary, pinned, warm_start)
    }
}

// ---------------------------------------------------------------------------
// Linearization
// ---------------------------------------------------------------------------

/// The numerically-linearized state-space model of a block at its
/// initial-condition operating point: `A = ∂ẋ/∂x`, `B = ∂ẋ/∂u`, `C = ∂y/∂x`,
/// `D = ∂y/∂u`, with states in `der()` order.
///
/// Port of `DynamicSolver.Linearization`.
#[derive(Debug, Clone, PartialEq)]
pub struct Linearization {
    pub states: Vec<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub a: Vec<Vec<f64>>,
    pub b: Vec<Vec<f64>>,
    pub c: Vec<Vec<f64>>,
    pub d: Vec<Vec<f64>>,
}

// ---------------------------------------------------------------------------
// The solver
// ---------------------------------------------------------------------------

/// Orchestrates one `DYNAMIC` block after the analytic solve: it builds the
/// vector RHS closure `f(t, y) → dy`, the event switching functions and the
/// sampled ODE Table, delegating the numerics to [`crate::ode::integrator`].
///
/// Port of `DynamicSolver`. Construct with [`DynamicSolver::new`], then call
/// [`solve`](DynamicSolver::solve) (or [`linearize`](DynamicSolver::linearize),
/// or [`dae_parts`](DynamicSolver::dae_parts)).
///
/// # Interior mutability, and why
///
/// The RHS and every switching function are closures handed to the driver, and
/// all of them run the per-step inner solve — which advances the warm start and
/// re-enters the caller's `AlgebraicSolve`. In Java those are `this::rhs`
/// method references mutating fields. Here they capture `&self` and the two
/// genuinely mutable pieces sit behind [`RefCell`]s, so several closures can
/// coexist inside one [`OdeProblem`].
pub struct DynamicSolver<'a> {
    system: &'a DynamicSystem,
    analytic_values: &'a Scope,
    defs: &'a Definitions,
    algebraic: RefCell<Box<dyn AlgebraicSolve + 'a>>,

    time_var: String,
    states: Vec<String>,
    state_set: HashSet<String>,
    aux_names: Vec<String>,
    algebraic_template: Vec<Equation>,
    event_bindings: Vec<EventBinding>,
    y0: Vec<f64>,
    warm_start: RefCell<Option<Scope>>,
}

impl<'a> DynamicSolver<'a> {
    /// Port of the Java constructor. `analytic_values` are the scalars the
    /// analytic solve resolved (parameters, initial-condition inputs); `defs`
    /// carries the document's `FUNCTION`/`TABLE` definitions for the evaluator.
    pub fn new(
        system: &'a DynamicSystem,
        analytic_values: &'a Scope,
        defs: &'a Definitions,
        algebraic: Box<dyn AlgebraicSolve + 'a>,
    ) -> DynamicSolver<'a> {
        DynamicSolver {
            system,
            analytic_values,
            defs,
            algebraic: RefCell::new(algebraic),
            time_var: system.options.time_var.clone(),
            states: Vec::new(),
            state_set: HashSet::new(),
            aux_names: Vec::new(),
            algebraic_template: Vec::new(),
            event_bindings: Vec::new(),
            y0: Vec::new(),
            warm_start: RefCell::new(None),
        }
    }

    // -- accessors the sibling modules need ---------------------------------

    /// The block's states, in `der()` order. Empty before classification.
    pub fn states(&self) -> &[String] {
        &self.states
    }

    /// The block's algebraic auxiliaries — the extra output columns.
    pub fn aux_names(&self) -> &[String] {
        &self.aux_names
    }

    /// The combined algebraic block solved at every step.
    pub fn algebraic_template(&self) -> &[Equation] {
        &self.algebraic_template
    }

    /// The initial state vector.
    pub fn y0(&self) -> &[f64] {
        &self.y0
    }

    /// The ODE table's column headers, `[timeVar, states…, auxiliaries…]`.
    pub fn columns(&self) -> Vec<String> {
        let mut columns = Vec::with_capacity(1 + self.states.len() + self.aux_names.len());
        columns.push(self.time_var.clone());
        columns.extend(self.states.iter().cloned());
        columns.extend(self.aux_names.iter().cloned());
        columns
    }

    // -- entry points -------------------------------------------------------

    /// Integrate the block and publish its ODE Table.
    ///
    /// Port of `DynamicSolver.solve()`.
    pub fn solve(&mut self) -> Result<OdeTableResult> {
        self.solve_with(crate::ode::integrator::integrate)
    }

    /// [`solve`](DynamicSolver::solve) against an explicit driver.
    ///
    /// The Java hard-codes `new OdeIntegrator().integrate(problem)`. Taking the
    /// driver as a parameter lets the orchestration be tested against a
    /// reference stepper without standing up the whole adaptive machinery, and
    /// costs nothing: [`solve`](DynamicSolver::solve) passes the real one.
    pub fn solve_with<I>(&mut self, integrate: I) -> Result<OdeTableResult>
    where
        I: FnOnce(&OdeProblem<'_>) -> Result<OdeResult>,
    {
        self.classify()?;
        let options = &self.system.options;
        if is_ida_method(&options.method) {
            return Err(FreesError::solver(format!(
                "DYNAMIC {}: method '{}' needs the implicit-DAE path, which this build does \
                 not provide yet. Pick a built-in method (ode45/ode23s/ode15s).",
                self.system.name, options.method
            )));
        }
        // Cap the step (default span/100) so the adaptive controller cannot grow
        // a single step large enough to step over an event — e.g. a high-altitude
        // near-vacuum coast where the dynamics are smooth would otherwise let one
        // giant step skip the apogee crossing and integrate into descent.
        let max_step = options
            .max_step
            .unwrap_or((options.tf - options.t0) / 100.0);

        let this: &DynamicSolver<'a> = self;
        let rhs = move |t: f64, y: &[f64]| this.rhs(t, y);
        let problem = OdeProblem {
            method: options.method.clone(),
            t0: options.t0,
            tf: options.tf,
            y0: this.y0.clone(),
            rhs: &rhs,
            points: options.points,
            fixed_step: options.step,
            rtol: options.rtol,
            atol: options.atol,
            max_step: Some(max_step),
            events: this.build_events(),
        };
        let result = integrate(&problem)?;
        this.build_table(&result)
    }

    /// Linearize the block about its initial-condition operating point by finite
    /// differences, reusing the per-step algebraic solve `ẋ = f(x, u)`:
    /// perturbing each state gives the `A` and `C` columns, each input the `B`
    /// and `D` columns. For a linear plant the result is exact at any point.
    ///
    /// Port of `DynamicSolver.linearize`. Inputs are exogenous values pinned in
    /// the analytic environment (e.g. a source value); outputs are any solved
    /// variables of the network (flat names).
    pub fn linearize(&mut self, inputs: &[String], outputs: &[String]) -> Result<Linearization> {
        if self.states.is_empty() {
            self.classify()?;
        }
        let t0 = self.system.options.t0;
        let n = self.states.len();
        let m = inputs.len();
        let p = outputs.len();
        let x0 = self.y0.clone();
        let base = self.solve_for_linearization(t0, &x0, &[])?;
        let f0 = self.der_values_of(&base);
        let y0v = self.output_values_of(&base, outputs)?;

        let mut a = vec![vec![0.0; n]; n];
        let mut c = vec![vec![0.0; n]; p];
        for j in 0..n {
            let eps = 1e-6 * java_max(x0[j].abs(), 1.0);
            let mut xp = x0.clone();
            xp[j] += eps;
            let v = self.solve_for_linearization(t0, &xp, &[])?;
            let fp = self.der_values_of(&v);
            let yp = self.output_values_of(&v, outputs)?;
            for (i, row) in a.iter_mut().enumerate() {
                row[j] = (fp[i] - f0[i]) / eps;
            }
            for (k, row) in c.iter_mut().enumerate() {
                row[j] = (yp[k] - y0v[k]) / eps;
            }
        }
        let mut b = vec![vec![0.0; m]; n];
        let mut d = vec![vec![0.0; m]; p];
        for (q, u) in inputs.iter().enumerate() {
            let u0 = self.analytic_values.get(u).copied().unwrap_or(0.0);
            let eps = 1e-6 * java_max(u0.abs(), 1.0);
            let v = self.solve_for_linearization(t0, &x0, &[(u.clone(), u0 + eps)])?;
            let fp = self.der_values_of(&v);
            let yp = self.output_values_of(&v, outputs)?;
            for (i, row) in b.iter_mut().enumerate() {
                row[q] = (fp[i] - f0[i]) / eps;
            }
            for (k, row) in d.iter_mut().enumerate() {
                row[q] = (yp[k] - y0v[k]) / eps;
            }
        }
        Ok(Linearization {
            states: self.states.clone(),
            inputs: inputs.to_vec(),
            outputs: outputs.to_vec(),
            a,
            b,
            c,
            d,
        })
    }

    fn solve_for_linearization(
        &self,
        t: f64,
        y: &[f64],
        overrides: &[(String, f64)],
    ) -> Result<Scope> {
        let mut pinned = self.pin_map(t, y);
        for (name, value) in overrides {
            pinned.insert(name.clone(), *value);
        }
        let ordered: Vec<(String, f64)> = pinned.into_iter().collect();
        let template = self.algebraic_template.clone();
        self.algebraic.borrow_mut().solve(&template, &ordered, None)
    }

    fn der_values_of(&self, values: &Scope) -> Vec<f64> {
        self.states
            .iter()
            .map(|s| values.get(&der_var(s)).copied().unwrap_or(0.0))
            .collect()
    }

    fn output_values_of(&self, values: &Scope, outputs: &[String]) -> Result<Vec<f64>> {
        let mut out = Vec::with_capacity(outputs.len());
        for name in outputs {
            match values.get(name) {
                Some(v) => out.push(*v),
                None => {
                    return Err(FreesError::solver(format!(
                        "LINEARIZE: output '{name}' is not a variable of the network '{}'.",
                        self.system.name
                    )))
                }
            }
        }
        Ok(out)
    }

    // -- the implicit-DAE assembly ------------------------------------------

    /// Everything [`crate::dae::assembly::assemble`] needs from this classified
    /// block.
    ///
    /// This is the front half of `DynamicSolver.assembleDae`: the squareness
    /// check with its full diagnostic, and the single inner algebraic solve at
    /// `t0` that seeds the auxiliaries and the state derivatives. A failure in
    /// that seed solve leaves `None`, so the consistent-initialization pass
    /// resolves them from zeros — exactly as the Java swallows the exception.
    ///
    /// The back half — residual, root function, sparsity, `id`, `y0`/`yp0` — is
    /// `dae/assembly.rs`, per the division of labour its module docs set out.
    /// Nothing is duplicated here.
    pub fn assembly_spec(&mut self) -> Result<AssemblySpec<'a>> {
        self.classify()?;
        let n = self.states.len() + self.aux_names.len();
        if self.algebraic_template.len() != n {
            return Err(FreesError::solver(self.non_square_diagnostic(n)));
        }
        let seed_y0 = self.y0.clone();
        let seed = self
            .solve_algebraic_at(self.system.options.t0, &seed_y0)
            .ok();
        Ok(AssemblySpec {
            block_name: self.system.name.clone(),
            time_var: self.time_var.clone(),
            states: self.states.clone(),
            aux: self.aux_names.clone(),
            template: self.algebraic_template.clone(),
            analytic_values: self.analytic_values.clone(),
            state_initials: self.y0.clone(),
            seed,
            events: self
                .event_bindings
                .iter()
                .map(|b| EventSpec {
                    name: b.name.clone(),
                    lhs: b.lhs.clone(),
                    rhs: b.rhs.clone(),
                    stops: b.stop,
                })
                .collect(),
            ctx: EvalContext::with_defs(self.defs),
        })
    }

    /// Assemble this block into the implicit DAE `F(t, y, y') = 0`.
    ///
    /// Port of `DynamicSolver.assembleDae`, delegating the assembly proper to
    /// [`crate::dae::assembly::assemble`]. The non-square case is caught here
    /// rather than there, because only this side can append the
    /// `Blocker.diagnose(probe)` sentence that names the exact hole — the gap
    /// `dae/assembly.rs` documents and asks the `DYNAMIC` owner to fill.
    ///
    /// # One guard the Java has and this does not
    ///
    /// The Java residual runs inside `PropertyFunctions.enterLenient()` so a
    /// stiff corrector probing off the fluid table clamps to a finite value
    /// instead of throwing. [`crate::props`] has no lenient mode, so such a
    /// probe propagates its error; `dae/solver.rs` treats that as a recoverable
    /// step failure and cuts the step, which is the same net behaviour for a
    /// transient excursion but not for a genuinely out-of-range model.
    pub fn assemble_dae(&mut self) -> Result<DaeAssembly<'a>> {
        let spec = self.assembly_spec()?;
        crate::dae::assembly::assemble(spec)
    }

    /// The de-sugared events, carrying the direction filter and `set`-action
    /// target that [`crate::dae::assembly::EventSpec`] does not model.
    ///
    /// The IDA path needs all three (the Java keeps them in `idaEventDirs`,
    /// `idaEventSetIdx` and `idaEventSetExpr`, aligned with the assembly's event
    /// order): a root crossing is only honoured when its direction matches, and
    /// a `set` action reassigns the state and re-initializes there.
    pub fn event_bindings(&self) -> &[EventBinding] {
        &self.event_bindings
    }

    // -- structural classification ------------------------------------------

    /// Split the expanded body into states, auxiliaries and the combined
    /// algebraic template, then validate the result.
    ///
    /// Port of `DynamicSolver.classify`. Idempotent per solver: `solve`,
    /// `linearize` and `dae_parts` all guard on `states.is_empty()` the way the
    /// Java does.
    fn classify(&mut self) -> Result<()> {
        if !self.states.is_empty() {
            return Ok(());
        }
        // Expand method-of-lines FOR loops and array indices against the solved
        // constants (e.g. N), turning der(T[i]) into concrete scalar states
        // der(T[1]), der(T[2]), … keyed as "t[1]", "t[2]", … — the same naming
        // the analytic array/FOR machinery uses.
        let mut aux_equations: Vec<Equation> = Vec::new();
        let der_rhs = self.collect_state_rhs(&mut aux_equations)?;

        let mut implicit_states: Vec<String> = Vec::new();
        let mut implicit_seen: HashSet<String> = HashSet::new();
        for eq in &aux_equations {
            collect_all_ders(&eq.lhs, &mut implicit_states, &mut implicit_seen);
            collect_all_ders(&eq.rhs, &mut implicit_states, &mut implicit_seen);
        }

        if der_rhs.is_empty() && implicit_states.is_empty() {
            return Err(FreesError::solver(format!(
                "DYNAMIC {}: no der(X) equation found — a DYNAMIC block needs at least one state.",
                self.system.name
            )));
        }

        for name in der_rhs.iter().map(|(name, _)| name) {
            self.states.push(name.clone());
            self.state_set.insert(name.clone());
        }
        for is in &implicit_states {
            if !self.state_set.contains(is) {
                self.states.push(is.clone());
                self.state_set.insert(is.clone());
            }
        }
        self.aux_names.retain(|a| !self.state_set.contains(a));
        self.initialize_state_vector()?;

        // Reify derivatives: der(X) -> der$X; build the combined algebraic block.
        for state in &self.states {
            if let Some((_, rhs)) = der_rhs.iter().find(|(name, _)| name == state) {
                self.algebraic_template.push(Equation::new(
                    Expr::var(der_var(state)),
                    substitute_der(rhs),
                    format!("der${state}"),
                ));
            }
        }
        for aux in &aux_equations {
            self.algebraic_template.push(Equation::new(
                substitute_der(&aux.lhs),
                substitute_der(&aux.rhs),
                aux.source_text.clone(),
            ));
        }
        self.register_implicit_auxiliaries();
        self.validate_references()?;
        self.check_structural_index()?;

        let states = self.states.clone();
        self.event_bindings = bind_events(&self.system.events, substitute_der, |name| {
            states.iter().position(|s| s == name)
        })?;
        Ok(())
    }

    /// Splits the expanded body into state RHS (`der` equations) and auxiliary
    /// equations; aux equations are appended to `aux_out` and their names
    /// recorded. Returns the per-state `der()` right-hand sides in insertion
    /// order — that order **is** the state ordering, and therefore the ODE
    /// table's column order.
    fn collect_state_rhs(&mut self, aux_out: &mut Vec<Equation>) -> Result<Vec<(String, Expr)>> {
        let mut der_rhs: Vec<(String, Expr)> = Vec::new();
        for eq in self.expand_body()? {
            let explicit_state = der_state_name(&eq.lhs);
            let mut rhs_ders = Vec::new();
            let mut seen = HashSet::new();
            collect_all_ders(&eq.rhs, &mut rhs_ders, &mut seen);
            match explicit_state {
                Some(state) if rhs_ders.is_empty() => {
                    if der_rhs.iter().any(|(name, _)| *name == state) {
                        return Err(FreesError::solver(format!(
                            "DYNAMIC {}: state '{state}' has more than one explicit der() equation.",
                            self.system.name
                        )));
                    }
                    der_rhs.push((state, eq.rhs));
                }
                _ => {
                    if let Some(aux_name) = simple_var_name(&eq.lhs) {
                        if !self.aux_names.contains(&aux_name) {
                            self.aux_names.push(aux_name);
                        }
                    }
                    aux_out.push(eq);
                }
            }
        }
        Ok(der_rhs)
    }

    /// Resolves one initial condition per state (array initials expand over
    /// their range) and populates the `y0` initial-state vector.
    fn initialize_state_vector(&mut self) -> Result<()> {
        let mut initial: BTreeMap<String, f64> = BTreeMap::new();
        for ic in &self.system.initials {
            self.expand_initial(ic, &mut initial)?;
        }
        for state in &self.states {
            if !initial.contains_key(state) {
                return Err(FreesError::solver(format!(
                    "DYNAMIC {}: state '{state}' has no initial condition ({state}({}) = …).",
                    self.system.name,
                    fmt_number(self.system.options.t0)
                )));
            }
        }
        self.y0 = self
            .states
            .iter()
            .map(|s| initial.get(s).copied().unwrap_or(0.0))
            .collect();
        Ok(())
    }

    /// Expands one initial condition (scalar, single element, or a `1:N` range)
    /// into per-state initial values.
    fn expand_initial(
        &self,
        ic: &InitialCondition,
        initial: &mut BTreeMap<String, f64>,
    ) -> Result<()> {
        // The Java `InitialCondition.state` is already lowercase (the AST builder
        // lowercases every identifier). Normalising here as well keeps a
        // hand-built AST honest without changing any parsed document.
        let state = ic.state.to_ascii_lowercase();
        if ic.indices.is_empty() {
            self.require_state(&state)?;
            initial.insert(state, self.eval(&ic.value, self.analytic_values)?);
            return Ok(());
        }
        if ic.indices.len() != 1 {
            return Err(FreesError::solver(format!(
                "DYNAMIC {}: multi-dimensional array initial conditions are not supported.",
                self.system.name
            )));
        }
        let no_loop = HashMap::new();
        let value = self.eval(&self.resolve(&ic.value, &no_loop)?, self.analytic_values)?;
        match &ic.indices[0] {
            Expr::Range { start, end } => {
                let lo = java_round(self.eval_index(start, &no_loop)?);
                let hi = java_round(self.eval_index(end, &no_loop)?);
                let step: i64 = if lo <= hi { 1 } else { -1 };
                let mut i = lo;
                while if lo <= hi { i <= hi } else { i >= hi } {
                    let key = format!("{state}[{i}]");
                    self.require_state(&key)?;
                    initial.insert(key, value);
                    i += step;
                }
            }
            index => {
                let key = format!("{state}[{}]", java_round(self.eval_index(index, &no_loop)?));
                self.require_state(&key)?;
                initial.insert(key, value);
            }
        }
        Ok(())
    }

    fn require_state(&self, name: &str) -> Result<()> {
        if !self.state_set.contains(name) {
            return Err(FreesError::solver(format!(
                "DYNAMIC {}: initial condition for '{name}' which is not a state \
                 (no der({name}) equation).",
                self.system.name
            )));
        }
        Ok(())
    }

    /// Promotes to auxiliaries every non-state variable the algebraic block
    /// references, not just simple `name = expr` assignment targets.
    ///
    /// An expanded component network defines variables *implicitly* through
    /// constraint equations (`a.Qdot + b.Qdot = 0`, `mass.T = wall.T`); these are
    /// still per-step unknowns the coupled algebraic solve determines, so they
    /// must be registered (and emitted as output columns) rather than rejected as
    /// undefined. A genuinely undefined reference instead makes the block
    /// non-square and surfaces at the per-step solve.
    fn register_implicit_auxiliaries(&mut self) {
        let mut known: HashSet<String> = self.analytic_values.keys().cloned().collect();
        known.insert(self.time_var.clone());
        known.insert("time".to_string()); // reserved global alias, pinned by pin_time
        for state in &self.states {
            known.insert(state.clone());
            known.insert(der_var(state));
        }
        let mut aux = self.aux_names.clone();
        let mut seen: HashSet<String> = aux.iter().cloned().collect();
        for eq in &self.algebraic_template {
            for side in [&eq.lhs, &eq.rhs] {
                for v in side.variables() {
                    if !known.contains(&v) && seen.insert(v.clone()) {
                        aux.push(v);
                    }
                }
            }
        }
        self.aux_names = aux;
    }

    /// Verifies that every variable the block reads is either a state, an
    /// auxiliary, the time variable, a reified derivative, or a parameter
    /// resolved by the analytic solve. A leftover would otherwise surface as a
    /// confusing "underspecified system" from the inner solve.
    ///
    /// # This check cannot fire, and that is faithful
    ///
    /// [`register_implicit_auxiliaries`](DynamicSolver::register_implicit_auxiliaries)
    /// runs immediately before it and promotes *every* otherwise-unknown name in
    /// the template to an auxiliary — over the identical `lhs.variables()` /
    /// `rhs.variables()` walk. So the set this scan tests is empty by
    /// construction, in the Java exactly as here. Confirmed against the oracle:
    /// a block whose body reads a name nothing defines reaches the per-step
    /// algebraic solve and fails there with the "underspecified" wrapper
    /// ([`solve_algebraic_at`](DynamicSolver::solve_algebraic_at)), never with
    /// this message. Kept because the Java keeps it — if a future change makes
    /// the promotion narrower, this is the guard that should catch it.
    fn validate_references(&self) -> Result<()> {
        let mut known: HashSet<String> = self.analytic_values.keys().cloned().collect();
        known.insert(self.time_var.clone());
        known.insert("time".to_string()); // reserved global alias, pinned by pin_time
        for state in &self.states {
            known.insert(state.clone());
            known.insert(der_var(state));
        }
        known.extend(self.aux_names.iter().cloned());
        let mut unknown: BTreeSet<String> = BTreeSet::new();
        for eq in &self.algebraic_template {
            for side in [&eq.lhs, &eq.rhs] {
                for v in side.variables() {
                    if !known.contains(&v) {
                        unknown.insert(v);
                    }
                }
            }
        }
        if unknown.is_empty() {
            return Ok(());
        }
        let listed: Vec<&str> = unknown.iter().map(String::as_str).collect();
        Err(FreesError::solver(format!(
            "DYNAMIC {}: references undefined variable(s) [{}]. They are not states, \
             auxiliaries, the time variable '{}', or parameters from the analytic solve. \
             If you meant the time variable, match the header ({} = t0 .. tf).",
            self.system.name,
            listed.join(", "),
            self.time_var,
            self.time_var
        )))
    }

    /// Structural index check.
    ///
    /// An index-1 DAE pairs every algebraic (non-`der`) equation with an
    /// algebraic unknown it can determine; the integrators here solve exactly
    /// that class. A constraint written directly between differentiated states —
    /// a rigid coupling like `w1 = w2` between two inertias, or an incompressible
    /// loop closure — leaves no algebraic unknown to pair with, makes the model
    /// index-2 or higher, and used to surface as an unexplained integrator
    /// failure at initialization. Detect it by maximum bipartite matching of the
    /// algebraic equations against the algebraic variables and name the culprits
    /// instead.
    ///
    /// Runs only when the assembly is square (otherwise the count diagnostic in
    /// [`dae_parts`](DynamicSolver::dae_parts) tells the better story) and flags
    /// only unmatched constraints that involve states.
    ///
    /// The Java uses JGraphT's Hopcroft–Karp; this uses Kuhn's augmenting-path
    /// algorithm over the same graph. Both compute a *maximum* matching, so the
    /// square/not-square verdict and the culprit **count** agree; which
    /// particular equations end up unmatched can differ between two maximum
    /// matchings, so the quoted lines in the message may differ from the Java's
    /// when several constraints are equally to blame.
    fn check_structural_index(&self) -> Result<()> {
        if self.algebraic_template.len() != self.states.len() + self.aux_names.len() {
            return Ok(());
        }
        let algebraic: Vec<&Equation> = self
            .algebraic_template
            .iter()
            .filter(|eq| !eq.variables().iter().any(|v| v.starts_with("der$")))
            .collect();
        if algebraic.is_empty() {
            return Ok(());
        }

        let aux_index: HashMap<&str, usize> = self
            .aux_names
            .iter()
            .enumerate()
            .map(|(i, a)| (a.as_str(), i))
            .collect();
        let adjacency: Vec<Vec<usize>> = algebraic
            .iter()
            .map(|eq| {
                let mut cols: Vec<usize> = eq
                    .variables()
                    .iter()
                    .filter_map(|v| aux_index.get(v.as_str()).copied())
                    .collect();
                cols.sort_unstable();
                cols.dedup();
                cols
            })
            .collect();
        let matched = maximum_matching(&adjacency, self.aux_names.len());

        let mut culprits: Vec<String> = Vec::new();
        for (i, eq) in algebraic.iter().enumerate() {
            if culprits.len() >= 4 {
                break;
            }
            if matched[i] {
                continue;
            }
            let coupled: Vec<String> = eq
                .variables()
                .into_iter()
                .filter(|v| self.state_set.contains(v))
                .collect();
            if coupled.is_empty() {
                continue; // no state involved — the count diagnostics own it
            }
            culprits.push(format!(
                "\"{}\" (couples {})",
                eq.source_text,
                display(&coupled)
            ));
        }
        if culprits.is_empty() {
            return Ok(());
        }
        let lead = if culprits.len() == 1 {
            "an algebraic constraint relates".to_string()
        } else {
            format!("{} algebraic constraints relate", culprits.len())
        };
        Err(FreesError::solver(format!(
            "DYNAMIC {}: the model is structurally index-2 or higher — {lead} differentiated \
             states with no algebraic unknown left to determine: {}. The integrators here solve \
             index-1 systems: differentiate the constraint, put a compliance/storage element \
             between the coupled states, or eliminate one state by substitution.",
            self.system.name,
            culprits.join("; ")
        )))
    }

    /// A non-square DAE assembly explained in the model's own vocabulary: which
    /// variables the network carries, how many equations it produced, and the
    /// usual physical cause. Display names, never flat internals.
    fn non_square_diagnostic(&self, n: usize) -> String {
        let produced = self.algebraic_template.len();
        let mut sb = format!(
            "DYNAMIC {}: the network's equation set is {} ({produced} equations for {n} unknowns: \
             {} state{} + {} algebraic).",
            self.system.name,
            if produced < n {
                "underdetermined"
            } else {
                "overdetermined"
            },
            self.states.len(),
            if self.states.len() == 1 { "" } else { "s" },
            self.aux_names.len()
        );
        if produced < n {
            sb.push_str(
                " A common cause: a branch has no flow-determining element — an efficiency-only \
                 machine or rigid pass-through feeding a storage volume leaves the through-flow \
                 free; add an orifice/valve/flow map, or pin a boundary flow.",
            );
        } else {
            sb.push_str(
                " A common cause: a boundary pins a quantity a component already defines (e.g. \
                 re-equating a mixer pressure or T-pinning a wall state).",
            );
        }
        sb.push_str(&format!(" States: {}.", display(&self.states)));
        // Name the exact hole: run the bipartite diagnosis over the template
        // with the states and time pinned (they are knowns per step).
        let mut probe = self.algebraic_template.clone();
        for state in &self.states {
            probe.push(Equation::new(
                Expr::var(state),
                Expr::num(0.0),
                format!("{state} [state]"),
            ));
        }
        probe.push(Equation::new(
            Expr::var(&self.time_var),
            Expr::num(0.0),
            format!("{} [time]", self.time_var),
        ));
        probe.push(Equation::new(
            Expr::var("time"),
            Expr::num(0.0),
            "time [time]",
        ));
        let diagnosis = structural_diagnosis(&probe);
        if !diagnosis.is_empty() {
            sb.push(' ');
            sb.push_str(&diagnosis);
        }
        sb
    }

    // -- method-of-lines expansion ------------------------------------------

    /// Top-level body equations plus every `FOR` loop expanded against the
    /// solved constants; array accesses become scalar `name[idx]` variables.
    fn expand_body(&self) -> Result<Vec<Equation>> {
        let mut out: Vec<Equation> = Vec::new();
        let no_loop: HashMap<String, f64> = HashMap::new();
        for eq in &self.system.body_equations {
            out.push(Equation::new(
                self.resolve(&eq.lhs, &no_loop)?,
                self.resolve(&eq.rhs, &no_loop)?,
                eq.source_text.clone(),
            ));
        }
        for fb in &self.system.for_blocks {
            self.expand_for(fb, &HashMap::new(), &mut out)?;
        }
        Ok(out)
    }

    fn expand_for(
        &self,
        block: &Statement,
        loop_vars: &HashMap<String, f64>,
        out: &mut Vec<Equation>,
    ) -> Result<()> {
        let Statement::For {
            var_name,
            start,
            end,
            body,
        } = block
        else {
            return Ok(());
        };
        let lo = java_round(self.eval_index(start, loop_vars)?);
        let hi = java_round(self.eval_index(end, loop_vars)?);
        let step: i64 = if lo <= hi { 1 } else { -1 };
        let mut i = lo;
        while if lo <= hi { i <= hi } else { i >= hi } {
            let mut lv = loop_vars.clone();
            lv.insert(var_name.to_ascii_lowercase(), i as f64);
            for st in body {
                match st {
                    Statement::Eq(eq) => out.push(Equation::new(
                        self.resolve(&eq.lhs, &lv)?,
                        self.resolve(&eq.rhs, &lv)?,
                        eq.source_text.clone(),
                    )),
                    inner @ Statement::For { .. } => self.expand_for(inner, &lv, out)?,
                    _ => {}
                }
            }
            if out.len() > 200_000 {
                return Err(FreesError::solver(format!(
                    "DYNAMIC {}: FOR expansion produced too many equations \
                     (reduce the node count).",
                    self.system.name
                )));
            }
            i += step;
        }
        Ok(())
    }

    /// Substitutes loop variables and lowers constant-index array accesses
    /// `T[expr]` to scalar variables `t[k]`.
    fn resolve(&self, e: &Expr, loop_vars: &HashMap<String, f64>) -> Result<Expr> {
        Ok(match e {
            Expr::Var(name) => match loop_vars.get(name) {
                Some(value) => Expr::num(*value),
                None => e.clone(),
            },
            Expr::ArrayAccess { name, indices } => {
                let mut key = String::from(name.as_str());
                key.push('[');
                for (k, index) in indices.iter().enumerate() {
                    if k > 0 {
                        key.push(',');
                    }
                    key.push_str(&java_round(self.eval_index(index, loop_vars)?).to_string());
                }
                key.push(']');
                Expr::var(key)
            }
            Expr::BinOp { op, left, right } => Expr::BinOp {
                op: *op,
                left: Box::new(self.resolve(left, loop_vars)?),
                right: Box::new(self.resolve(right, loop_vars)?),
            },
            Expr::Neg(operand) => Expr::Neg(Box::new(self.resolve(operand, loop_vars)?)),
            Expr::Call { function, args } => {
                let mut mapped = Vec::with_capacity(args.len());
                for a in args {
                    mapped.push(self.resolve(a, loop_vars)?);
                }
                Expr::Call {
                    function: function.clone(),
                    args: mapped,
                }
            }
            Expr::Compare { op, left, right } => Expr::Compare {
                op: *op,
                left: Box::new(self.resolve(left, loop_vars)?),
                right: Box::new(self.resolve(right, loop_vars)?),
            },
            Expr::Logical { op, left, right } => Expr::Logical {
                op: *op,
                left: Box::new(self.resolve(left, loop_vars)?),
                right: Box::new(self.resolve(right, loop_vars)?),
            },
            Expr::Not(operand) => Expr::Not(Box::new(self.resolve(operand, loop_vars)?)),
            other => other.clone(),
        })
    }

    fn eval_index(&self, e: &Expr, loop_vars: &HashMap<String, f64>) -> Result<f64> {
        let mut env = self.analytic_values.clone();
        env.extend(loop_vars.iter().map(|(k, v)| (k.clone(), *v)));
        self.eval(&self.resolve(e, loop_vars)?, &env)
    }

    // -- RHS closure (one shared step cursor across all states) --------------

    fn rhs(&self, t: f64, y: &[f64]) -> Result<Vec<f64>> {
        let values = self.solve_algebraic_at(t, y)?;
        let mut dy = Vec::with_capacity(self.states.len());
        for state in &self.states {
            match values.get(&der_var(state)) {
                Some(d) => dy.push(*d),
                None => {
                    return Err(FreesError::solver(format!(
                        "DYNAMIC {}: failed to resolve der({state}) at {} = {t}",
                        self.system.name, self.time_var
                    )))
                }
            }
        }
        Ok(dy)
    }

    /// The pinned environment for one step: every analytic value, then time,
    /// then the state vector.
    ///
    /// The two time insertions are `DynamicSolver.pinTime`: the block's declared
    /// time variable **and** the reserved global `time` (the name component
    /// bodies use) are both pinned to the integrator's current time, with the
    /// alias skipped when the document itself defines `time`, so a legacy
    /// variable of that name keeps its meaning. (The Java's other `pinTime`
    /// caller, `daeValues`, is now `crate::dae::assembly::dae_values`, which
    /// carries the same two lines.)
    ///
    /// A `BTreeMap` because the pins become `var = value` equations and a
    /// duplicate would make the subsystem overspecified — map semantics are
    /// required, and sorted order makes the equation list reproducible (the
    /// Java's `LinkedHashMap` inherits `HashMap` iteration order from
    /// `analyticValues`, which is unspecified).
    fn pin_map(&self, t: f64, y: &[f64]) -> BTreeMap<String, f64> {
        let mut m: BTreeMap<String, f64> = self
            .analytic_values
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        m.insert(self.time_var.clone(), t);
        m.entry("time".to_string()).or_insert(t);
        for (k, state) in self.states.iter().enumerate() {
            m.insert(state.clone(), y[k]);
        }
        m
    }

    /// Solve the algebraic block with time and the state vector pinned.
    fn solve_algebraic_at(&self, t: f64, y: &[f64]) -> Result<Scope> {
        let pinned: Vec<(String, f64)> = self.pin_map(t, y).into_iter().collect();
        let outcome = {
            let warm = self.warm_start.borrow();
            let mut algebraic = self.algebraic.borrow_mut();
            algebraic.solve(&self.algebraic_template, &pinned, warm.as_ref())
        };
        match outcome {
            Ok(values) => {
                let copy = values.clone();
                *self.warm_start.borrow_mut() = Some(values);
                Ok(copy)
            }
            Err(FreesError::Solver { message })
                if message.contains("underspecified")
                    || message.contains("structurally singular") =>
            {
                // Same vocabulary as the DAE diagnostic: name the block and the
                // usual physical cause instead of leaking a bare count.
                //
                // The Java interpolates the raw `double`, so the time reads
                // `0.0`, not `0`. Borrowing the existing JVM spelling helper
                // rather than adding a fourth copy of it.
                Err(FreesError::solver(format!(
                    "DYNAMIC {} (per-step algebraic solve at {} = {}): {message} A common \
                     cause: a branch with no flow-determining element (an efficiency-only \
                     machine or rigid pass-through leaves its through-flow or a port pressure \
                     free) — add an orifice/valve/flow map, pin a boundary, or use method = ida \
                     for genuinely derivative-coupled networks.",
                    self.system.name,
                    self.time_var,
                    crate::props::hx::java_double_to_string(t)
                )))
            }
            Err(other) => Err(other),
        }
    }

    // -- events --------------------------------------------------------------

    /// Port of `DynamicSolver.buildEvents`: turn each binding into a switching
    /// function that runs the per-step inner solve and evaluates `lhs - rhs`.
    fn build_events(&self) -> Vec<OdeEvent<'_>> {
        let mut out = Vec::with_capacity(self.event_bindings.len());
        for binding in &self.event_bindings {
            let lhs = binding.lhs.clone();
            let rhs = binding.rhs.clone();
            let g = move |t: f64, y: &[f64]| {
                let values = self.solve_algebraic_at(t, y)?;
                Ok(self.eval(&lhs, &values)? - self.eval(&rhs, &values)?)
            };
            let set = match (binding.set_index, binding.set_expr.clone()) {
                (Some(index), Some(expr)) => {
                    let value = move |t: f64, y: &[f64]| {
                        let values = self.solve_algebraic_at(t, y)?;
                        self.eval(&expr, &values)
                    };
                    Some(StateReset {
                        index,
                        value: Box::new(value),
                    })
                }
                _ => None,
            };
            out.push(OdeEvent {
                name: binding.name.clone(),
                g: Box::new(g),
                direction: binding.direction,
                stop: binding.stop,
                set,
            });
        }
        out
    }

    // -- ODE Table assembly --------------------------------------------------

    /// Port of `DynamicSolver.buildTable`: re-solve the algebraic block at every
    /// sampled time so the auxiliary columns are consistent with the state
    /// trajectory the driver returned.
    fn build_table(&self, result: &OdeResult) -> Result<OdeTableResult> {
        let columns = self.columns();
        let mut rows: Vec<Vec<f64>> = Vec::with_capacity(result.times.len());
        for (i, &t) in result.times.iter().enumerate() {
            let yi = &result.states[i];
            let values = self.solve_algebraic_at(t, yi)?;
            let mut row = Vec::with_capacity(columns.len());
            row.push(t);
            row.extend(yi.iter().take(self.states.len()).copied());
            for aux in &self.aux_names {
                // Always present: `register_implicit_auxiliaries` made every
                // auxiliary a variable of the template the inner solve returns.
                row.push(values.get(aux).copied().unwrap_or(f64::NAN));
            }
            rows.push(row);
        }
        Ok(OdeTableResult {
            name: self.system.name.clone(),
            columns,
            rows,
            events: result
                .events
                .iter()
                .map(|er| TableEventHit {
                    name: er.name.clone(),
                    time: er.time,
                })
                .collect(),
            method: self.system.options.method.clone(),
            stopped: result.stopped,
            end_time: result.end_time,
        })
    }

    fn eval(&self, expr: &Expr, scope: &Scope) -> Result<f64> {
        eval_with(expr, scope, EvalContext::with_defs(self.defs))
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Whether a method name routes to the implicit-DAE path rather than an
/// explicit integrator. Port of `DynamicSolver.isIdaMethod`.
pub fn is_ida_method(method: &str) -> bool {
    matches!(
        method.to_ascii_lowercase().as_str(),
        "ida" | "idas" | "ida15s" | "dae"
    )
}

/// The reified unknown standing for `der(state)`. Port of `DynamicSolver.derVar`.
pub fn der_var(state: &str) -> String {
    format!("der${state}")
}

/// If `lhs` is `der(stateVar)`, the state name; otherwise `None`.
///
/// Port of `DynamicSolver.derStateName`. Deliberately stricter than
/// [`crate::ode::analysis::der_state_name`]: it matches only a bare
/// [`Expr::Var`] argument, because by the time `classify` runs every
/// `der(T[i])` has already been lowered to `der(t[3])` by
/// [`DynamicSolver::resolve`].
pub fn der_state_name(lhs: &Expr) -> Option<String> {
    match lhs {
        Expr::Call { function, args } if function == "der" && args.len() == 1 => match &args[0] {
            Expr::Var(name) => Some(name.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn simple_var_name(lhs: &Expr) -> Option<String> {
    match lhs {
        Expr::Var(name) => Some(name.clone()),
        _ => None,
    }
}

/// Rewrites every `der(X)` call to the reified unknown `der$X`.
///
/// Port of `DynamicSolver.substituteDer`. Like the Java it does **not** descend
/// into array subscripts — by this point `resolve` has already turned every
/// `ArrayAccess` into a flat `Expr::Var`.
pub fn substitute_der(e: &Expr) -> Expr {
    match e {
        Expr::Call { function, args } => {
            if function == "der" && args.len() == 1 {
                if let Expr::Var(name) = &args[0] {
                    return Expr::var(der_var(name));
                }
            }
            Expr::Call {
                function: function.clone(),
                args: args.iter().map(substitute_der).collect(),
            }
        }
        Expr::BinOp { op, left, right } => Expr::BinOp {
            op: *op,
            left: Box::new(substitute_der(left)),
            right: Box::new(substitute_der(right)),
        },
        Expr::Neg(operand) => Expr::Neg(Box::new(substitute_der(operand))),
        Expr::Compare { op, left, right } => Expr::Compare {
            op: *op,
            left: Box::new(substitute_der(left)),
            right: Box::new(substitute_der(right)),
        },
        Expr::Logical { op, left, right } => Expr::Logical {
            op: *op,
            left: Box::new(substitute_der(left)),
            right: Box::new(substitute_der(right)),
        },
        Expr::Not(operand) => Expr::Not(Box::new(substitute_der(operand))),
        other => other.clone(),
    }
}

/// Every `der(X)` mentioned anywhere in `e`, in first-seen order.
///
/// Port of `DynamicSolver.collectAllDers`. Note which variants it walks: calls,
/// binary operators, negation, comparisons, logicals, `not`, and an array
/// access's *indices*. Literals, ranges and array literals are leaves, as in the
/// Java `if`/`else if` chain that has no arm for them.
fn collect_all_ders(e: &Expr, found: &mut Vec<String>, seen: &mut HashSet<String>) {
    match e {
        Expr::Call { function, args } => {
            if function == "der" && args.len() == 1 {
                if let Expr::Var(name) = &args[0] {
                    if seen.insert(name.clone()) {
                        found.push(name.clone());
                    }
                }
            }
            for a in args {
                collect_all_ders(a, found, seen);
            }
        }
        Expr::BinOp { left, right, .. }
        | Expr::Compare { left, right, .. }
        | Expr::Logical { left, right, .. } => {
            collect_all_ders(left, found, seen);
            collect_all_ders(right, found, seen);
        }
        Expr::Neg(operand) | Expr::Not(operand) => collect_all_ders(operand, found, seen),
        Expr::ArrayAccess { indices, .. } => {
            for index in indices {
                collect_all_ders(index, found, seen);
            }
        }
        _ => {}
    }
}

/// Flat solver names → dotted display names. Port of `DynamicSolver.display`.
fn display(names: &[String]) -> String {
    names
        .iter()
        .map(|v| v.replace('$', "."))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The structural hole in an equation list, in the blocker's own words.
///
/// Stands in for the Java `Blocker.diagnose(equations)`, which is a public entry
/// onto the private causality diagnosis. The Rust blocker keeps that diagnosis
/// private but reaches it on exactly the failing paths, so probing
/// [`crate::solver::blocker::block_system`] with no knowns recovers the same
/// sentence. A probe that blocks cleanly yields no diagnosis, where the Java
/// would still describe the (square, matchable) system.
fn structural_diagnosis(equations: &[Equation]) -> String {
    match crate::solver::blocker::block_system(equations, &HashSet::new()) {
        Ok(_) => String::new(),
        Err(err) => err.to_string_message(),
    }
}

/// Kuhn's augmenting-path maximum bipartite matching. Returns, per left node
/// (equation), whether it is matched.
fn maximum_matching(adjacency: &[Vec<usize>], right_count: usize) -> Vec<bool> {
    fn augment(
        left: usize,
        adjacency: &[Vec<usize>],
        seen: &mut [bool],
        right_to_left: &mut [Option<usize>],
    ) -> bool {
        for &right in &adjacency[left] {
            if seen[right] {
                continue;
            }
            seen[right] = true;
            let free = right_to_left[right];
            if free.is_none() || augment(free.unwrap(), adjacency, seen, right_to_left) {
                right_to_left[right] = Some(left);
                return true;
            }
        }
        false
    }

    let mut right_to_left: Vec<Option<usize>> = vec![None; right_count];
    let mut matched = vec![false; adjacency.len()];
    for (left, slot) in matched.iter_mut().enumerate() {
        let mut seen = vec![false; right_count];
        *slot = augment(left, adjacency, &mut seen, &mut right_to_left);
    }
    matched
}

/// `Math.round(double)` — round half **up** (towards positive infinity), unlike
/// Rust's `f64::round`, which rounds half away from zero.
fn java_round(v: f64) -> i64 {
    if v.is_nan() {
        return 0;
    }
    (v + 0.5).floor() as i64
}

/// `Math.max(double, double)` — NaN-propagating, unlike Rust's `f64::max`.
fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a >= b {
        a
    } else {
        b
    }
}

/// `DynamicSolver.fmt`: an integral value prints without a decimal point.
fn fmt_number(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BinOp, CmpOp};
    use crate::ode::problem::EventRecord;

    // -- test harness -------------------------------------------------------

    /// A minimal stand-in for the engine's `solve_pinned`: the algebraic
    /// template of the documents below is always explicit (`x = expr`, in
    /// dependency order after the `der$` rows), so one forward substitution pass
    /// per equation, repeated until it converges, resolves it exactly.
    fn substitution_solver(
        ordinary: &[Equation],
        pinned: &[(String, f64)],
        _warm: Option<&Scope>,
    ) -> Result<Scope> {
        let mut values: Scope = pinned.iter().cloned().collect();
        let defs = Definitions::default();
        for _ in 0..(ordinary.len() + 2) {
            let mut progressed = false;
            for eq in ordinary {
                let Expr::Var(target) = &eq.lhs else {
                    continue;
                };
                if values.contains_key(target) {
                    continue;
                }
                if let Ok(v) = eval_with(&eq.rhs, &values, EvalContext::with_defs(&defs)) {
                    values.insert(target.clone(), v);
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }
        for eq in ordinary {
            if let Expr::Var(target) = &eq.lhs {
                if !values.contains_key(target) {
                    return Err(FreesError::solver(format!(
                        "the system is underspecified: '{target}' has no value"
                    )));
                }
            }
        }
        Ok(values)
    }

    /// Fixed-step RK4 with linear event bracketing — enough to exercise the
    /// orchestration (`solve_with`) without the real adaptive driver.
    fn reference_integrator(problem: &OdeProblem<'_>) -> Result<OdeResult> {
        let samples = problem.sample_count();
        let mut times = Vec::with_capacity(samples);
        let mut states: Vec<Vec<f64>> = Vec::with_capacity(samples);
        let mut events = Vec::new();
        let span = problem.tf - problem.t0;
        let h = span / (samples - 1) as f64;

        let mut t = problem.t0;
        let mut y = problem.y0.clone();
        let mut g_prev: Vec<f64> = problem
            .events
            .iter()
            .map(|e| e.g.eval(t, &y))
            .collect::<Result<_>>()?;
        times.push(t);
        states.push(y.clone());

        for _ in 1..samples {
            let (t_next, mut y_next) = rk4(problem, t, &y, h)?;
            let mut stop_at: Option<(f64, Vec<f64>)> = None;
            for (r, event) in problem.events.iter().enumerate() {
                let g_new = event.g.eval(t_next, &y_next)?;
                if !event.triggers(g_prev[r], g_new) {
                    g_prev[r] = g_new;
                    continue;
                }
                // Linear bracket, exact for the linear switching functions the
                // tests below use.
                let denom = g_prev[r] - g_new;
                let frac = if denom == 0.0 { 0.0 } else { g_prev[r] / denom };
                let t_hit = t + frac * (t_next - t);
                let y_hit: Vec<f64> = y
                    .iter()
                    .zip(&y_next)
                    .map(|(a, b)| a + frac * (b - a))
                    .collect();
                events.push(EventRecord {
                    name: event.name.clone(),
                    time: t_hit,
                    state: y_hit.clone(),
                });
                if event.stop {
                    stop_at = Some((t_hit, y_hit));
                    break;
                }
                if let Some(reset) = &event.set {
                    // The discrete latch: overwrite the state at the crossing and
                    // integrate the remainder of the step from the modified state.
                    let mut y_reset = y_hit.clone();
                    y_reset[reset.index] = reset.value.eval(t_hit, &y_hit)?;
                    let (_, y_rest) = rk4(problem, t_hit, &y_reset, t_next - t_hit)?;
                    y_next = y_rest;
                }
                g_prev[r] = event.g.eval(t_next, &y_next)?;
            }
            if let Some((t_hit, y_hit)) = stop_at {
                times.push(t_hit);
                states.push(y_hit);
                return Ok(OdeResult {
                    times,
                    states,
                    events,
                    stopped: true,
                    end_time: t_hit,
                    accepted_steps: 0,
                    rejected_steps: 0,
                });
            }
            t = t_next;
            y = y_next;
            times.push(t);
            states.push(y.clone());
        }
        Ok(OdeResult {
            times,
            states,
            events,
            stopped: false,
            end_time: problem.tf,
            accepted_steps: 0,
            rejected_steps: 0,
        })
    }

    fn rk4(problem: &OdeProblem<'_>, t: f64, y: &[f64], h: f64) -> Result<(f64, Vec<f64>)> {
        let add = |y: &[f64], k: &[f64], f: f64| -> Vec<f64> {
            y.iter().zip(k).map(|(a, b)| a + f * b).collect()
        };
        let k1 = problem.rhs.eval(t, y)?;
        let k2 = problem.rhs.eval(t + h / 2.0, &add(y, &k1, h / 2.0))?;
        let k3 = problem.rhs.eval(t + h / 2.0, &add(y, &k2, h / 2.0))?;
        let k4 = problem.rhs.eval(t + h, &add(y, &k3, h))?;
        let y_next = (0..y.len())
            .map(|i| y[i] + h / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]))
            .collect();
        Ok((t + h, y_next))
    }

    fn options(tf: f64, points: usize) -> DynamicOptions {
        DynamicOptions {
            points: Some(points),
            ..DynamicOptions::defaults("time", 0.0, tf)
        }
    }

    fn eq(lhs: Expr, rhs: Expr, text: &str) -> Equation {
        Equation::new(lhs, rhs, text)
    }

    fn der(state: &str) -> Expr {
        Expr::call("der", vec![Expr::var(state)])
    }

    /// Newton cooling: `der(Temp) = -k * (Temp - Tinf)`, `Temp(0) = 95`.
    fn cooling(points: usize) -> DynamicSystem {
        DynamicSystem {
            name: "cool".into(),
            options: options(60.0, points),
            body_equations: vec![eq(
                der("temp"),
                Expr::Neg(Box::new(Expr::bin(
                    BinOp::Mul,
                    Expr::var("k"),
                    Expr::bin(BinOp::Sub, Expr::var("temp"), Expr::var("tinf")),
                ))),
                "der(Temp) = -k*(Temp - Tinf)",
            )],
            for_blocks: Vec::new(),
            initials: vec![InitialCondition {
                state: "temp".into(),
                indices: Vec::new(),
                value: Expr::num(95.0),
            }],
            events: Vec::new(),
            source_text: "DYNAMIC cool(...) ... END".into(),
        }
    }

    fn cooling_values() -> Scope {
        let mut v = Scope::default();
        v.insert("k".into(), 0.05);
        v.insert("tinf".into(), 20.0);
        v
    }

    fn solver<'a>(
        system: &'a DynamicSystem,
        values: &'a Scope,
        defs: &'a Definitions,
    ) -> DynamicSolver<'a> {
        DynamicSolver::new(system, values, defs, Box::new(substitution_solver))
    }

    // -- classification ------------------------------------------------------

    #[test]
    fn a_block_with_no_der_is_refused() {
        let mut system = cooling(5);
        system.body_equations = vec![eq(Expr::var("q"), Expr::num(1.0), "Q = 1")];
        system.initials.clear();
        let values = cooling_values();
        let defs = Definitions::default();
        let err = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap_err()
            .to_string_message();
        assert!(err.contains("no der(X) equation found"), "{err}");
    }

    #[test]
    fn two_der_equations_for_one_state_are_refused() {
        let mut system = cooling(5);
        system
            .body_equations
            .push(eq(der("temp"), Expr::num(1.0), "der(Temp) = 1"));
        let values = cooling_values();
        let defs = Definitions::default();
        let err = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap_err()
            .to_string_message();
        assert!(
            err.contains("state 'temp' has more than one explicit der() equation"),
            "{err}"
        );
    }

    #[test]
    fn a_state_without_an_initial_condition_is_refused() {
        let mut system = cooling(5);
        system.initials.clear();
        let values = cooling_values();
        let defs = Definitions::default();
        let err = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap_err()
            .to_string_message();
        assert!(
            err.contains("state 'temp' has no initial condition"),
            "{err}"
        );
        // `fmt` drops the decimal point on an integral t0.
        assert!(err.contains("temp(0) = …"), "{err}");
    }

    #[test]
    fn an_initial_condition_for_a_non_state_is_refused() {
        let mut system = cooling(5);
        system.initials.push(InitialCondition {
            state: "q".into(),
            indices: Vec::new(),
            value: Expr::num(1.0),
        });
        let values = cooling_values();
        let defs = Definitions::default();
        let err = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap_err()
            .to_string_message();
        assert!(err.contains("initial condition for 'q'"), "{err}");
        assert!(err.contains("no der(q) equation"), "{err}");
    }

    #[test]
    fn an_undefined_reference_becomes_an_auxiliary_and_fails_at_the_per_step_solve() {
        // Oracle `dyn_err_undefined_ref`: the Java reports "DYNAMIC undef
        // (per-step algebraic solve at time = 0.0): … underspecified … Free
        // quantity (no defining relation): nope", *not* `validateReferences`'s
        // "references undefined variable(s)" — see that method's docs for why it
        // is unreachable in both engines.
        let mut system = cooling(5);
        system.body_equations[0] = eq(der("temp"), Expr::var("nope"), "der(Temp) = nope");
        let values = cooling_values();
        let defs = Definitions::default();
        let mut s = solver(&system, &values, &defs);
        s.classify().unwrap();
        assert_eq!(s.aux_names(), ["nope"]);
        let err = s
            .solve_with(reference_integrator)
            .unwrap_err()
            .to_string_message();
        assert!(
            err.contains("DYNAMIC cool (per-step algebraic solve at time = 0.0)"),
            "{err}"
        );
        assert!(!err.contains("references undefined variable(s)"), "{err}");
    }

    #[test]
    fn the_time_variable_and_the_reserved_global_are_both_known() {
        // Header declares `t`; the body reads the reserved global `time`.
        let mut system = cooling(5);
        system.options.time_var = "t".into();
        system.body_equations = vec![eq(der("temp"), Expr::var("time"), "der(Temp) = time")];
        let values = cooling_values();
        let defs = Definitions::default();
        let table = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap();
        assert_eq!(table.columns, vec!["t", "temp"]);
        // dTemp/dt = t  =>  Temp(60) = 95 + 60^2/2 = 1895.
        let last = table.rows.last().unwrap();
        assert!((last[1] - 1895.0).abs() < 1e-6, "{last:?}");
    }

    #[test]
    fn auxiliaries_become_columns_after_the_states() {
        let mut system = cooling(3);
        system.body_equations.push(eq(
            Expr::var("qdot"),
            Expr::bin(
                BinOp::Mul,
                Expr::var("k"),
                Expr::bin(BinOp::Sub, Expr::var("temp"), Expr::var("tinf")),
            ),
            "Qdot = k*(Temp - Tinf)",
        ));
        let values = cooling_values();
        let defs = Definitions::default();
        let table = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap();
        assert_eq!(table.columns, vec!["time", "temp", "qdot"]);
        // Qdot at t = 0 is k*(95 - 20) = 3.75.
        assert!((table.rows[0][2] - 3.75).abs() < 1e-12);
    }

    #[test]
    fn an_implicitly_defined_variable_is_registered_as_an_auxiliary() {
        // `0 = qdot - k*(Temp - Tinf)` never assigns `qdot` on the left, so only
        // `register_implicit_auxiliaries` can surface it as a column.
        let mut system = cooling(2);
        system.body_equations.push(eq(
            Expr::bin(
                BinOp::Sub,
                Expr::var("qdot"),
                Expr::bin(
                    BinOp::Mul,
                    Expr::var("k"),
                    Expr::bin(BinOp::Sub, Expr::var("temp"), Expr::var("tinf")),
                ),
            ),
            Expr::num(0.0),
            "Qdot - k*(Temp - Tinf) = 0",
        ));
        let values = cooling_values();
        let defs = Definitions::default();
        let mut s = solver(&system, &values, &defs);
        s.classify().unwrap();
        assert_eq!(s.aux_names(), ["qdot"]);
        assert_eq!(s.columns(), vec!["time", "temp", "qdot"]);
    }

    #[test]
    fn an_implicit_state_is_discovered_from_a_der_on_the_right() {
        // `0 = der(v) - a` has no `der(...)` LHS, so `v` can only become a state
        // through the implicit-state scan.
        let system = DynamicSystem {
            name: "imp".into(),
            options: options(1.0, 2),
            body_equations: vec![eq(
                Expr::num(0.0),
                Expr::bin(BinOp::Sub, der("v"), Expr::var("a")),
                "0 = der(v) - a",
            )],
            for_blocks: Vec::new(),
            initials: vec![InitialCondition {
                state: "v".into(),
                indices: Vec::new(),
                value: Expr::num(0.0),
            }],
            events: Vec::new(),
            source_text: String::new(),
        };
        let mut values = Scope::default();
        values.insert("a".into(), 2.0);
        let defs = Definitions::default();
        let mut s = solver(&system, &values, &defs);
        s.classify().unwrap();
        assert_eq!(s.states(), ["v"]);
        assert!(s.aux_names().is_empty());
    }

    #[test]
    fn a_rigid_coupling_between_states_is_reported_as_index_two() {
        // Two inertias with `W1 = W2` written directly, plus a torque split so
        // the assembly is square (2 states + 2 algebraic = 4 equations) and the
        // index check actually runs — it returns early on a non-square block and
        // lets the count diagnostics own the story.
        //
        // Oracle `dyn_err_index2_square` produces exactly this sentence, quoting
        // the whitespace-stripped source `"W1=W2"` that ANTLR's `getText()`
        // yields.
        let system = DynamicSystem {
            name: "rigid".into(),
            options: options(10.0, 3),
            body_equations: vec![
                eq(der("w1"), Expr::var("tq1"), "der(W1) = Tq1"),
                eq(der("w2"), Expr::var("tq2"), "der(W2) = Tq2"),
                eq(Expr::var("w1"), Expr::var("w2"), "W1=W2"),
                eq(
                    Expr::bin(BinOp::Add, Expr::var("tq1"), Expr::var("tq2")),
                    Expr::var("tin"),
                    "Tq1+Tq2=Tin",
                ),
            ],
            for_blocks: Vec::new(),
            initials: vec![
                InitialCondition {
                    state: "w1".into(),
                    indices: Vec::new(),
                    value: Expr::num(0.0),
                },
                InitialCondition {
                    state: "w2".into(),
                    indices: Vec::new(),
                    value: Expr::num(0.0),
                },
            ],
            events: Vec::new(),
            source_text: String::new(),
        };
        let mut values = Scope::default();
        values.insert("tin".into(), 10.0);
        let defs = Definitions::default();
        let err = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap_err()
            .to_string_message();
        assert_eq!(
            err,
            "DYNAMIC rigid: the model is structurally index-2 or higher — an algebraic \
             constraint relates differentiated states with no algebraic unknown left to \
             determine: \"W1=W2\" (couples w1, w2). The integrators here solve index-1 systems: \
             differentiate the constraint, put a compliance/storage element between the coupled \
             states, or eliminate one state by substitution."
        );
    }

    #[test]
    fn a_non_square_block_skips_the_index_check() {
        // The same rigid coupling without the torque split: 3 equations for
        // 2 states + 0 auxiliaries, so `check_structural_index` returns early
        // and the failure surfaces from the per-step solve instead. Oracle
        // `dyn_err_index2` confirms the Java behaves the same way.
        let system = DynamicSystem {
            name: "rigid".into(),
            options: options(10.0, 3),
            body_equations: vec![
                eq(der("w1"), Expr::var("tq"), "der(W1) = Tq"),
                eq(der("w2"), Expr::var("tq"), "der(W2) = Tq"),
                eq(Expr::var("w1"), Expr::var("w2"), "W1=W2"),
            ],
            for_blocks: Vec::new(),
            initials: vec![
                InitialCondition {
                    state: "w1".into(),
                    indices: Vec::new(),
                    value: Expr::num(0.0),
                },
                InitialCondition {
                    state: "w2".into(),
                    indices: Vec::new(),
                    value: Expr::num(0.0),
                },
            ],
            events: Vec::new(),
            source_text: String::new(),
        };
        let mut values = Scope::default();
        values.insert("tq".into(), 1.0);
        let defs = Definitions::default();
        let mut s = solver(&system, &values, &defs);
        s.classify().unwrap();
        assert_eq!(s.states(), ["w1", "w2"]);
        assert!(s.aux_names().is_empty());
        assert_eq!(s.algebraic_template().len(), 3);
    }

    // -- method of lines -----------------------------------------------------

    #[test]
    fn a_for_block_expands_array_states_against_the_solved_constants() {
        // der(T[i]) = i for i = 1..3, T[1:3](0) = 0.
        let system = DynamicSystem {
            name: "mol".into(),
            options: options(1.0, 2),
            body_equations: Vec::new(),
            for_blocks: vec![Statement::For {
                var_name: "i".into(),
                start: Expr::num(1.0),
                end: Expr::var("n"),
                body: vec![Statement::Eq(eq(
                    Expr::call(
                        "der",
                        vec![Expr::ArrayAccess {
                            name: "t".into(),
                            indices: vec![Expr::var("i")],
                        }],
                    ),
                    Expr::var("i"),
                    "der(T[i]) = i",
                ))],
            }],
            initials: vec![InitialCondition {
                state: "t".into(),
                indices: vec![Expr::Range {
                    start: Box::new(Expr::num(1.0)),
                    end: Box::new(Expr::var("n")),
                }],
                value: Expr::num(0.0),
            }],
            events: Vec::new(),
            source_text: String::new(),
        };
        let mut values = Scope::default();
        values.insert("n".into(), 3.0);
        let defs = Definitions::default();
        let mut s = solver(&system, &values, &defs);
        s.classify().unwrap();
        assert_eq!(s.states(), ["t[1]", "t[2]", "t[3]"]);
        assert_eq!(s.y0(), [0.0, 0.0, 0.0]);
        let table = s.solve_with(reference_integrator).unwrap();
        assert_eq!(table.columns, vec!["time", "t[1]", "t[2]", "t[3]"]);
        let last = table.rows.last().unwrap();
        assert!((last[1] - 1.0).abs() < 1e-9, "{last:?}");
        assert!((last[3] - 3.0).abs() < 1e-9, "{last:?}");
    }

    #[test]
    fn a_single_element_array_initial_targets_one_state() {
        let system = DynamicSystem {
            name: "mol2".into(),
            options: options(1.0, 2),
            body_equations: Vec::new(),
            for_blocks: vec![Statement::For {
                var_name: "i".into(),
                start: Expr::num(1.0),
                end: Expr::num(2.0),
                body: vec![Statement::Eq(eq(
                    Expr::call(
                        "der",
                        vec![Expr::ArrayAccess {
                            name: "t".into(),
                            indices: vec![Expr::var("i")],
                        }],
                    ),
                    Expr::num(0.0),
                    "der(T[i]) = 0",
                ))],
            }],
            initials: vec![
                InitialCondition {
                    state: "t".into(),
                    indices: vec![Expr::num(1.0)],
                    value: Expr::num(7.0),
                },
                InitialCondition {
                    state: "t".into(),
                    indices: vec![Expr::num(2.0)],
                    value: Expr::num(9.0),
                },
            ],
            events: Vec::new(),
            source_text: String::new(),
        };
        let values = Scope::default();
        let defs = Definitions::default();
        let mut s = solver(&system, &values, &defs);
        s.classify().unwrap();
        assert_eq!(s.y0(), [7.0, 9.0]);
    }

    #[test]
    fn a_multi_dimensional_array_initial_is_refused() {
        let mut system = cooling(2);
        system.initials = vec![InitialCondition {
            state: "temp".into(),
            indices: vec![Expr::num(1.0), Expr::num(2.0)],
            value: Expr::num(0.0),
        }];
        let values = cooling_values();
        let defs = Definitions::default();
        let err = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap_err()
            .to_string_message();
        assert!(
            err.contains("multi-dimensional array initial conditions are not supported"),
            "{err}"
        );
    }

    // -- integration ---------------------------------------------------------

    #[test]
    fn the_plain_ode_matches_the_analytic_solution() {
        // Temp(t) = 20 + 75*exp(-0.05 t). This one runs on the reference RK4 so
        // it exercises `solve_with` independently of the production driver;
        // `oracle_plain_ode_newton_cooling` is the parity check. 61 samples give
        // h = 1, where the fourth-order error is ~1e-8 relative.
        let system = cooling(61);
        let values = cooling_values();
        let defs = Definitions::default();
        let table = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap();
        assert_eq!(table.name, "cool");
        assert_eq!(table.columns, vec!["time", "temp"]);
        assert_eq!(table.method, "ode45");
        assert!(!table.stopped);
        assert_eq!(table.end_time, 60.0);
        assert_eq!(table.rows.len(), 61);
        for row in &table.rows {
            let expected = 20.0 + 75.0 * (-0.05 * row[0]).exp();
            assert!(
                (row[1] - expected).abs() <= 1e-6 * expected,
                "{row:?} vs {expected}"
            );
        }
        assert_eq!(table.rows[0][1], 95.0);
    }

    #[test]
    fn a_stop_event_ends_the_run_at_the_crossing() {
        let mut system = cooling(61);
        system.events = vec![DynamicEvent::new(
            "cold",
            Expr::var("temp"),
            Expr::num(50.0),
            Some("falling".into()),
            "stop",
        )];
        let values = cooling_values();
        let defs = Definitions::default();
        let table = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap();
        assert!(table.stopped);
        assert_eq!(table.events.len(), 1);
        assert_eq!(table.events[0].name, "cold");
        // 20 + 75 exp(-0.05 t) = 50  =>  t = ln(2.5)/0.05 = 18.3258…
        assert!((table.events[0].time - 18.325814637483102).abs() < 1e-2);
        assert_eq!(table.end_time, table.events[0].time);
        let last = table.rows.last().unwrap();
        assert!((last[1] - 50.0).abs() < 1e-3, "{last:?}");
    }

    #[test]
    fn a_rising_filter_ignores_a_falling_crossing() {
        let mut system = cooling(61);
        system.events = vec![DynamicEvent::new(
            "never",
            Expr::var("temp"),
            Expr::num(50.0),
            Some("rising".into()),
            "stop",
        )];
        let values = cooling_values();
        let defs = Definitions::default();
        let table = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap();
        assert!(!table.stopped);
        assert!(table.events.is_empty());
    }

    #[test]
    fn a_set_event_reassigns_the_state_and_restarts() {
        // der(Temp) = 0 with Temp(0) = 1; an event on `time = 4.5` latches Temp
        // to 10. The threshold sits *between* two samples on purpose: the
        // reference stepper below brackets linearly and, unlike the production
        // driver, has no guard against re-firing on a crossing that lands
        // exactly on a step boundary.
        let system = DynamicSystem {
            name: "latch".into(),
            options: options(10.0, 11),
            body_equations: vec![eq(der("temp"), Expr::num(0.0), "der(Temp) = 0")],
            for_blocks: Vec::new(),
            initials: vec![InitialCondition {
                state: "temp".into(),
                indices: Vec::new(),
                value: Expr::num(1.0),
            }],
            events: vec![DynamicEvent {
                name: "trip".into(),
                lhs: Expr::var("time"),
                rhs: Expr::num(4.5),
                direction: Some("rising".into()),
                action: "set".into(),
                set_var: Some("temp".into()),
                set_expr: Some(Expr::num(10.0)),
            }],
            source_text: String::new(),
        };
        let values = Scope::default();
        let defs = Definitions::default();
        let table = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap();
        assert_eq!(table.events.len(), 1);
        assert_eq!(table.events[0].name, "trip");
        assert!(
            (table.events[0].time - 4.5).abs() < 1e-9,
            "{:?}",
            table.events
        );
        assert!(!table.stopped);
        // Before the latch the state is 1, after it 10.
        assert_eq!(table.rows[0][1], 1.0);
        assert_eq!(table.rows[4][1], 1.0);
        assert_eq!(table.rows.last().unwrap()[1], 10.0);
    }

    #[test]
    fn a_set_target_that_is_not_a_state_is_refused() {
        let mut system = cooling(3);
        system.events = vec![DynamicEvent {
            name: "bad".into(),
            lhs: Expr::var("temp"),
            rhs: Expr::num(50.0),
            direction: None,
            action: "set".into(),
            set_var: Some("k".into()),
            set_expr: Some(Expr::num(0.0)),
        }];
        let values = cooling_values();
        let defs = Definitions::default();
        let err = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap_err()
            .to_string_message();
        assert!(err.contains("set target 'k' is not a state"), "{err}");
    }

    #[test]
    fn an_event_switching_function_may_read_an_auxiliary() {
        let mut system = cooling(61);
        system.body_equations.push(eq(
            Expr::var("qdot"),
            Expr::bin(
                BinOp::Mul,
                Expr::var("k"),
                Expr::bin(BinOp::Sub, Expr::var("temp"), Expr::var("tinf")),
            ),
            "Qdot = k*(Temp - Tinf)",
        ));
        system.events = vec![DynamicEvent::new(
            "weak",
            Expr::var("qdot"),
            Expr::num(1.5),
            Some("falling".into()),
            "record",
        )];
        let values = cooling_values();
        let defs = Definitions::default();
        let table = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap();
        assert_eq!(table.events.len(), 1);
        // Qdot = 0.05*(Temp-20) = 1.5  =>  Temp = 50  =>  t = ln(2.5)/0.05.
        assert!((table.events[0].time - 18.325814637483102).abs() < 1e-2);
        assert!(!table.stopped);
    }

    // -- against the Java oracle ---------------------------------------------
    //
    // These drive the *real* driver (`solve`, i.e. `ode::integrator::integrate`)
    // and compare against tables dumped from the Java engine by
    // `tools/golden-dumper/run.sh`. Every literal below is an oracle value, not
    // an analytic approximation.

    /// Relative comparison at the parity harness's tolerance.
    fn assert_rows(actual: &[Vec<f64>], expected: &[&[f64]], tol: f64, what: &str) {
        assert_eq!(actual.len(), expected.len(), "{what}: row count");
        for (i, (got, want)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(got.len(), want.len(), "{what}: row {i} width");
            for (j, (g, w)) in got.iter().zip(want.iter()).enumerate() {
                let scale = w.abs().max(1.0);
                assert!(
                    (g - w).abs() <= tol * scale,
                    "{what}: row {i} col {j}: {g} vs oracle {w}"
                );
            }
        }
    }

    #[test]
    fn oracle_plain_ode_newton_cooling() {
        // fixtures probe `dyn_plain_ode`:
        //   k = 0.05 / Tinf = 20 / der(Temp) = -k*(Temp - Tinf) / Temp(0) = 95
        //   DYNAMIC cooling (method = ode45, time = 0 .. 60, points = 4)
        let mut system = cooling(4);
        system.name = "cooling".into();
        let values = cooling_values();
        let defs = Definitions::default();
        let t = solver(&system, &values, &defs).solve().unwrap();
        assert_eq!(t.name, "cooling");
        assert_eq!(t.columns, vec!["time", "temp"]);
        assert_eq!(t.method, "ode45");
        assert!(!t.stopped);
        assert_eq!(t.end_time, 60.0);
        assert!(t.events.is_empty());
        assert_rows(
            &t.rows,
            &[
                &[0.0, 95.0],
                &[20.0, 47.59095803046333],
                &[40.0, 30.15014623853744],
                &[60.0, 23.734030127668667],
            ],
            1e-9,
            "dyn_plain_ode",
        );
    }

    #[test]
    fn oracle_auxiliary_column() {
        // probe `dyn_aux_column`: der(Temp) = -Qdot / Qdot = k*(Temp - Tinf).
        // The auxiliary is re-solved at every sampled time, so its column is
        // consistent with the state trajectory.
        let mut system = cooling(4);
        system.name = "cooling".into();
        system.body_equations = vec![
            eq(
                der("temp"),
                Expr::Neg(Box::new(Expr::var("qdot"))),
                "der(Temp)=-Qdot",
            ),
            eq(
                Expr::var("qdot"),
                Expr::bin(
                    BinOp::Mul,
                    Expr::var("k"),
                    Expr::bin(BinOp::Sub, Expr::var("temp"), Expr::var("tinf")),
                ),
                "Qdot=k*(Temp-Tinf)",
            ),
        ];
        let values = cooling_values();
        let defs = Definitions::default();
        let t = solver(&system, &values, &defs).solve().unwrap();
        assert_eq!(t.columns, vec!["time", "temp", "qdot"]);
        assert_rows(
            &t.rows,
            &[
                &[0.0, 95.0, 3.75],
                &[20.0, 47.59095803046333, 1.3795479015231664],
                &[40.0, 30.15014623853744, 0.5075073119268722],
                &[60.0, 23.734030127668667, 0.18670150638343339],
            ],
            1e-9,
            "dyn_aux_column",
        );
    }

    #[test]
    fn oracle_stop_event() {
        // probe `dyn_event_stop`: EVENT cold: Temp = 50 | falling -> stop,
        // 7 points over 0 .. 60. The run ends at the crossing and the samples
        // are redistributed over the shortened span.
        let mut system = cooling(7);
        system.name = "coolstop".into();
        system.events = vec![DynamicEvent::new(
            "cold",
            Expr::var("temp"),
            Expr::num(50.0),
            Some("falling".into()),
            "stop",
        )];
        let values = cooling_values();
        let defs = Definitions::default();
        let t = solver(&system, &values, &defs).solve().unwrap();
        assert!(t.stopped);
        assert_eq!(t.events.len(), 1);
        assert_eq!(t.events[0].name, "cold");
        assert!(
            (t.events[0].time - 18.325814613429316).abs() < 1e-9,
            "{}",
            t.events[0].time
        );
        assert!((t.end_time - 18.325814613429316).abs() < 1e-9);
        assert_rows(
            &t.rows,
            &[
                &[0.0, 95.0],
                &[3.0543024355715525, 84.37806633936742],
                &[6.108604871143105, 75.26047239748533],
                &[9.162907306714658, 67.4341648316547],
                &[12.21720974228621, 60.71626419873782],
                &[15.271512177857764, 54.9497914976165],
                &[18.325814613429316, 50.0],
            ],
            1e-9,
            "dyn_event_stop",
        );
    }

    #[test]
    fn oracle_record_event_does_not_stop() {
        // probe `dyn_event_record`: der(Level) = 1 with EVENT half: Level = 5.
        let system = DynamicSystem {
            name: "ramp".into(),
            options: options(10.0, 6),
            body_equations: vec![eq(der("level"), Expr::num(1.0), "der(Level)=1")],
            for_blocks: Vec::new(),
            initials: vec![InitialCondition {
                state: "level".into(),
                indices: Vec::new(),
                value: Expr::num(0.0),
            }],
            events: vec![DynamicEvent::new(
                "half",
                Expr::var("level"),
                Expr::num(5.0),
                Some("rising".into()),
                "record",
            )],
            source_text: String::new(),
        };
        let values = Scope::default();
        let defs = Definitions::default();
        let t = solver(&system, &values, &defs).solve().unwrap();
        assert!(!t.stopped);
        assert_eq!(t.end_time, 10.0);
        assert_eq!(t.events.len(), 1);
        assert_eq!(t.events[0].name, "half");
        assert!(
            (t.events[0].time - 5.0).abs() < 1e-9,
            "{}",
            t.events[0].time
        );
        assert_rows(
            &t.rows,
            &[
                &[0.0, 0.0],
                &[2.0, 1.9999999999999998],
                &[4.0, 3.9999999999999996],
                &[6.0, 6.000000000000001],
                &[8.0, 8.000000000000002],
                &[10.0, 10.0],
            ],
            1e-9,
            "dyn_event_record",
        );
    }

    #[test]
    fn oracle_set_event_latches_and_restarts() {
        // probe `dyn_event_set`: der(Level) = 1 with
        // EVENT trip: Level = 4 | rising -> set Level = 0.
        // A sawtooth — the crossing reassigns the state and integration
        // restarts from the modified value, twice over the span.
        let system = DynamicSystem {
            name: "latch".into(),
            options: options(10.0, 11),
            body_equations: vec![eq(der("level"), Expr::num(1.0), "der(Level)=1")],
            for_blocks: Vec::new(),
            initials: vec![InitialCondition {
                state: "level".into(),
                indices: Vec::new(),
                value: Expr::num(0.0),
            }],
            events: vec![DynamicEvent {
                name: "trip".into(),
                lhs: Expr::var("level"),
                rhs: Expr::num(4.0),
                direction: Some("rising".into()),
                action: "set".into(),
                set_var: Some("level".into()),
                set_expr: Some(Expr::num(0.0)),
            }],
            source_text: String::new(),
        };
        let values = Scope::default();
        let defs = Definitions::default();
        let t = solver(&system, &values, &defs).solve().unwrap();
        assert!(!t.stopped);
        assert_eq!(t.end_time, 10.0);
        assert_eq!(
            t.events.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["trip", "trip"]
        );
        assert!((t.events[0].time - 4.0).abs() < 1e-9, "{:?}", t.events);
        assert!(
            (t.events[1].time - 7.999999999999983).abs() < 1e-8,
            "{:?}",
            t.events
        );
        assert_rows(
            &t.rows,
            &[
                &[0.0, 0.0],
                &[1.0, 1.0],
                &[2.0, 1.9999999999999998],
                &[3.0, 3.0000000000000004],
                &[4.0, 0.0],
                &[5.0, 1.0000000000000036],
                &[6.0, 2.0000000000000075],
                &[7.0, 3.000000000000012],
                &[8.0, 1.687538997430238e-14],
                &[9.0, 1.0000000000000195],
                &[10.0, 2.0000000000000235],
            ],
            1e-8,
            "dyn_event_set",
        );
    }

    #[test]
    fn oracle_method_of_lines_array_states() {
        // probe `dyn_method_of_lines`: FOR i = 1 TO N: der(Node[i]) =
        // alpha*(Hot - Node[i]); Node[1:3](0) = 20.
        let system = DynamicSystem {
            name: "bar".into(),
            options: options(50.0, 6),
            body_equations: Vec::new(),
            for_blocks: vec![Statement::For {
                var_name: "i".into(),
                start: Expr::num(1.0),
                end: Expr::var("n"),
                body: vec![Statement::Eq(eq(
                    Expr::call(
                        "der",
                        vec![Expr::ArrayAccess {
                            name: "node".into(),
                            indices: vec![Expr::var("i")],
                        }],
                    ),
                    Expr::bin(
                        BinOp::Mul,
                        Expr::var("alpha"),
                        Expr::bin(
                            BinOp::Sub,
                            Expr::var("hot"),
                            Expr::ArrayAccess {
                                name: "node".into(),
                                indices: vec![Expr::var("i")],
                            },
                        ),
                    ),
                    "der(Node[i])=alpha*(Hot-Node[i])",
                ))],
            }],
            initials: vec![InitialCondition {
                state: "node".into(),
                indices: vec![Expr::Range {
                    start: Box::new(Expr::num(1.0)),
                    end: Box::new(Expr::num(3.0)),
                }],
                value: Expr::num(20.0),
            }],
            events: Vec::new(),
            source_text: String::new(),
        };
        let mut values = Scope::default();
        values.insert("n".into(), 3.0);
        values.insert("alpha".into(), 0.01);
        values.insert("hot".into(), 100.0);
        let defs = Definitions::default();
        let t = solver(&system, &values, &defs).solve().unwrap();
        assert_eq!(t.columns, vec!["time", "node[1]", "node[2]", "node[3]"]);
        assert_rows(
            &t.rows,
            &[
                &[0.0, 20.0, 20.0, 20.0],
                &[10.0, 27.6130065571271, 27.6130065571271, 27.6130065571271],
                &[
                    20.0,
                    34.501539753764966,
                    34.501539753764966,
                    34.501539753764966,
                ],
                &[
                    30.0,
                    40.73454234546575,
                    40.73454234546575,
                    40.73454234546575,
                ],
                &[
                    40.0,
                    46.37439631715177,
                    46.37439631715177,
                    46.37439631715177,
                ],
                &[
                    50.0,
                    51.47754722298938,
                    51.47754722298938,
                    51.47754722298938,
                ],
            ],
            1e-9,
            "dyn_method_of_lines",
        );
    }

    #[test]
    fn oracle_time_alias_through_the_real_driver() {
        // probe `dyn_time_alias`: header declares `t`, body reads the reserved
        // global `time`. Columns are headed by the *declared* name.
        let system = DynamicSystem {
            name: "alias".into(),
            options: DynamicOptions {
                points: Some(5),
                ..DynamicOptions::defaults("t", 0.0, 4.0)
            },
            body_equations: vec![eq(der("y"), Expr::var("time"), "der(Y)=time")],
            for_blocks: Vec::new(),
            initials: vec![InitialCondition {
                state: "y".into(),
                indices: Vec::new(),
                value: Expr::num(0.0),
            }],
            events: Vec::new(),
            source_text: String::new(),
        };
        let values = Scope::default();
        let defs = Definitions::default();
        let t = solver(&system, &values, &defs).solve().unwrap();
        assert_eq!(t.columns, vec!["t", "y"]);
        assert_rows(
            &t.rows,
            &[
                &[0.0, 0.0],
                &[1.0, 0.49999999999999967],
                &[2.0, 1.9999999999999973],
                &[3.0, 4.499999999999996],
                &[4.0, 7.999999999999992],
            ],
            1e-9,
            "dyn_time_alias",
        );
    }

    #[test]
    fn oracle_accessors_read_the_solved_table() {
        // probe `dyn_accessor_read`: the analytic system reads the 21-point
        // cooling table. The five values below are the Java's, and they are
        // reproduced here by integrating with the real driver and then running
        // `ode::accessors::compute` over the resulting table — the same two
        // steps the second-solve pass performs.
        let system = cooling(21);
        let values = cooling_values();
        let defs = Definitions::default();
        let t = solver(&system, &values, &defs).solve().unwrap();
        assert_eq!(t.rows.len(), 21);
        let acc =
            |f: &str, arg: Option<f64>| crate::ode::accessors::compute(&t, f, "temp", arg).unwrap();
        let close = |got: f64, want: f64, what: &str| {
            assert!(
                (got - want).abs() <= 1e-9 * want.abs().max(1.0),
                "{what}: {got} vs oracle {want}"
            );
        };
        close(acc("finalvalue", None), 23.734030127668667, "Tlast");
        close(
            acc("maxvalue", None) - acc("minvalue", None),
            71.26596987233134,
            "Tspan",
        );
        close(acc("odevalue", Some(30.0)), 36.734761996461124, "Thalf");
        close(acc("timeat", Some(50.0)), 18.34801895160751, "Tcross");
        close(acc("odeavg", None), 44.54114148753955, "Tmean");
    }

    #[test]
    fn oracle_live_accessor_second_solve_pass() {
        // probe `dyn_accessor_live`: the analytic constraint
        // `FinalValue('Temp') = 30` sizes the ODE input `k`, which the oracle
        // solves to 0.033581717009069624 with the table ending at
        // 29.99999999999999. Driving the bridge with a one-dimensional secant
        // iteration stands in for the analytic Newton here; what is under test
        // is that the context re-integrates per iterate, caches per signature,
        // and reads the fresh table back.
        let system = cooling(21);
        let defs = Definitions::default();
        let systems = std::slice::from_ref(&system);
        let integrations = std::cell::Cell::new(0usize);
        let ctx = crate::ode::accessors::DynamicAccessorContext::install(
            systems,
            BTreeMap::new(),
            Box::new(|ds: &DynamicSystem, v: &Scope| {
                integrations.set(integrations.get() + 1);
                DynamicSolver::new(ds, v, &defs, Box::new(substitution_solver)).solve()
            }),
        );
        let residual = |k: f64| -> f64 {
            let mut v = Scope::default();
            v.insert("k".into(), k);
            v.insert("tinf".into(), 20.0);
            ctx.resolve("finalvalue", "temp", None, &v).unwrap() - 30.0
        };
        let (mut a, mut b) = (0.01_f64, 0.06_f64);
        let (mut fa, mut fb) = (residual(a), residual(b));
        for _ in 0..60 {
            let c = b - fb * (b - a) / (fb - fa);
            a = b;
            fa = fb;
            b = c;
            fb = residual(b);
            if fb.abs() < 1e-11 {
                break;
            }
        }
        assert!(
            (b - 0.033581717009069624).abs() < 1e-9,
            "k = {b} vs oracle 0.033581717009069624"
        );
        // Two probes at the same k reuse one integration (the signature cache).
        let before = integrations.get();
        residual(b);
        residual(b);
        assert_eq!(integrations.get(), before, "cached by input signature");
    }

    // -- diagnostics ---------------------------------------------------------

    #[test]
    fn a_per_step_underspecified_solve_names_the_block_and_the_time() {
        // `q` is referenced but never determined: the inner solve reports
        // "underspecified", which the wrapper must re-word.
        let mut system = cooling(3);
        system.body_equations[0] = eq(
            der("temp"),
            Expr::bin(BinOp::Add, Expr::var("q"), Expr::var("r")),
            "der(Temp) = Q + R",
        );
        system
            .body_equations
            .push(eq(Expr::var("q"), Expr::var("r"), "Q = R"));
        let values = cooling_values();
        let defs = Definitions::default();
        let err = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap_err()
            .to_string_message();
        // Oracle `dyn_err_undefined_ref` spells the time the JVM way: `0.0`.
        assert!(
            err.contains("DYNAMIC cool (per-step algebraic solve at time = 0.0)"),
            "{err}"
        );
        assert!(err.contains("flow-determining element"), "{err}");
    }

    #[test]
    fn an_ida_method_is_refused_rather_than_silently_integrated() {
        let mut system = cooling(3);
        system.options.method = "ida".into();
        let values = cooling_values();
        let defs = Definitions::default();
        let err = solver(&system, &values, &defs)
            .solve_with(reference_integrator)
            .unwrap_err()
            .to_string_message();
        assert!(err.contains("implicit-DAE path"), "{err}");
        for name in ["ida", "IDAS", "Ida15s", "dae"] {
            assert!(is_ida_method(name), "{name}");
        }
        for name in ["ode45", "ode23s", "ode15s", ""] {
            assert!(!is_ida_method(name), "{name}");
        }
    }

    // -- DAE assembly --------------------------------------------------------

    #[test]
    fn the_dae_assembly_is_square_and_seeded() {
        let mut system = cooling(3);
        system.body_equations.push(eq(
            Expr::var("qdot"),
            Expr::bin(
                BinOp::Mul,
                Expr::var("k"),
                Expr::bin(BinOp::Sub, Expr::var("temp"), Expr::var("tinf")),
            ),
            "Qdot = k*(Temp - Tinf)",
        ));
        let values = cooling_values();
        let defs = Definitions::default();
        let mut s = solver(&system, &values, &defs);
        let dae = s.assemble_dae().unwrap();
        assert_eq!(dae.n, 2);
        assert_eq!(dae.variables, vec!["temp", "qdot"]);
        assert_eq!(dae.id, vec![1.0, 0.0]);
        assert_eq!(dae.y0[0], 95.0);
        // Seeded from the inner solve at t0: Qdot = 0.05*(95-20) = 3.75,
        // der(Temp) = -3.75.
        assert!((dae.y0[1] - 3.75).abs() < 1e-12);
        assert!((dae.yp0[0] + 3.75).abs() < 1e-12);
        // Row 0 is `der$temp = -(k*(temp - tinf))`: touches temp (col 0) only.
        assert_eq!(dae.sparsity[0], vec![0]);
        // Row 1 is `qdot = k*(temp - tinf)`: touches qdot and temp.
        assert_eq!(dae.sparsity[1], vec![0, 1]);
        // The residual is zero at the seeded consistent point.
        let mut res = vec![0.0; dae.n];
        dae.residual.eval(0.0, &dae.y0, &dae.yp0, &mut res).unwrap();
        assert!(res.iter().all(|r| r.abs() < 1e-12), "{res:?}");
    }

    #[test]
    fn a_non_square_assembly_explains_itself() {
        // `Qdot` appears but nothing determines it: 1 equation, 2 unknowns.
        let mut system = cooling(3);
        system.body_equations[0] = eq(
            der("temp"),
            Expr::Neg(Box::new(Expr::var("qdot"))),
            "der(Temp) = -Qdot",
        );
        let values = cooling_values();
        let defs = Definitions::default();
        let mut s = solver(&system, &values, &defs);
        let err = s.assemble_dae().unwrap_err().to_string_message();
        assert!(err.contains("underdetermined"), "{err}");
        assert!(err.contains("1 equations for 2 unknowns"), "{err}");
        assert!(err.contains("1 state + 1 algebraic"), "{err}");
        assert!(err.contains("States: temp."), "{err}");
        // The sentence `dae/assembly.rs` cannot produce: the exact hole, from
        // the blocker, over the probe with the states and time pinned.
        assert!(
            err.contains("underspecified") || err.contains("structurally singular"),
            "the Blocker.diagnose sentence is missing: {err}"
        );
    }

    #[test]
    fn dae_event_metadata_is_carried_through() {
        let mut system = cooling(3);
        system.events = vec![
            DynamicEvent::new(
                "cold",
                Expr::var("temp"),
                Expr::num(50.0),
                Some("falling".into()),
                "stop",
            ),
            DynamicEvent {
                name: "latch".into(),
                lhs: Expr::var("temp"),
                rhs: Expr::num(30.0),
                direction: None,
                action: "set".into(),
                set_var: Some("temp".into()),
                set_expr: Some(Expr::num(31.0)),
            },
        ];
        let values = cooling_values();
        let defs = Definitions::default();
        let mut s = solver(&system, &values, &defs);
        let dae = s.assemble_dae().unwrap();
        assert_eq!(dae.event_names, vec!["cold", "latch"]);
        assert_eq!(dae.event_stops, vec![true, false]);
        assert_eq!(dae.event_count(), 2);
        // Roots read `temp - 50` and `temp - 30` at the seeded point.
        let mut roots = vec![0.0; 2];
        dae.root_fn
            .as_ref()
            .unwrap()
            .eval(0.0, &dae.y0, &dae.yp0, &mut roots)
            .unwrap();
        assert_eq!(roots, vec![45.0, 65.0]);

        // The direction filter and the `set` target are *not* in `DaeAssembly`;
        // the IDA path reads them off the bindings (Java `idaEventDirs` /
        // `idaEventSetIdx` / `idaEventSetExpr`), aligned with the same order.
        let bindings = s.event_bindings();
        assert_eq!(
            bindings.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
            ["cold", "latch"]
        );
        assert_eq!(
            bindings.iter().map(|b| b.direction).collect::<Vec<_>>(),
            [-1, 0]
        );
        assert_eq!(
            bindings.iter().map(|b| b.set_index).collect::<Vec<_>>(),
            [None, Some(0)]
        );
        assert_eq!(bindings[1].set_expr, Some(Expr::num(31.0)));
    }

    // -- linearization --------------------------------------------------------

    #[test]
    fn linearize_recovers_the_state_space_of_a_first_order_plant() {
        // der(Temp) = (Qin - k*(Temp - Tinf)) / C, output Qdot.
        let system = DynamicSystem {
            name: "plant".into(),
            options: options(1.0, 2),
            body_equations: vec![
                eq(
                    der("temp"),
                    Expr::bin(
                        BinOp::Div,
                        Expr::bin(
                            BinOp::Sub,
                            Expr::var("qin"),
                            Expr::bin(
                                BinOp::Mul,
                                Expr::var("k"),
                                Expr::bin(BinOp::Sub, Expr::var("temp"), Expr::var("tinf")),
                            ),
                        ),
                        Expr::var("c"),
                    ),
                    "der(Temp) = (Qin - k*(Temp - Tinf))/C",
                ),
                eq(
                    Expr::var("qdot"),
                    Expr::bin(
                        BinOp::Mul,
                        Expr::var("k"),
                        Expr::bin(BinOp::Sub, Expr::var("temp"), Expr::var("tinf")),
                    ),
                    "Qdot = k*(Temp - Tinf)",
                ),
            ],
            for_blocks: Vec::new(),
            initials: vec![InitialCondition {
                state: "temp".into(),
                indices: Vec::new(),
                value: Expr::num(20.0),
            }],
            events: Vec::new(),
            source_text: String::new(),
        };
        let mut values = Scope::default();
        values.insert("k".into(), 2.0);
        values.insert("c".into(), 4.0);
        values.insert("tinf".into(), 20.0);
        values.insert("qin".into(), 0.0);
        let defs = Definitions::default();
        let lin = solver(&system, &values, &defs)
            .linearize(&["qin".to_string()], &["qdot".to_string()])
            .unwrap();
        assert_eq!(lin.states, ["temp"]);
        assert!((lin.a[0][0] + 0.5).abs() < 1e-6, "{:?}", lin.a); // -k/C
        assert!((lin.b[0][0] - 0.25).abs() < 1e-6, "{:?}", lin.b); // 1/C
        assert!((lin.c[0][0] - 2.0).abs() < 1e-6, "{:?}", lin.c); // k
        assert!(lin.d[0][0].abs() < 1e-9, "{:?}", lin.d);
    }

    #[test]
    fn linearize_names_an_output_that_is_not_in_the_network() {
        let system = cooling(2);
        let values = cooling_values();
        let defs = Definitions::default();
        let err = solver(&system, &values, &defs)
            .linearize(&[], &["nope".to_string()])
            .unwrap_err()
            .to_string_message();
        assert!(err.contains("LINEARIZE: output 'nope'"), "{err}");
        assert!(err.contains("network 'cool'"), "{err}");
    }

    // -- pure helpers ---------------------------------------------------------

    #[test]
    fn substitute_der_reifies_only_der_of_a_bare_variable() {
        assert_eq!(substitute_der(&der("x")), Expr::var("der$x"));
        // Nested inside arithmetic.
        let e = Expr::bin(BinOp::Add, der("x"), Expr::var("y"));
        assert_eq!(
            substitute_der(&e),
            Expr::bin(BinOp::Add, Expr::var("der$x"), Expr::var("y"))
        );
        // `der` of something that is not a plain variable is left alone but its
        // arguments are still walked.
        let arr = Expr::call(
            "der",
            vec![Expr::ArrayAccess {
                name: "t".into(),
                indices: vec![Expr::num(1.0)],
            }],
        );
        assert_eq!(substitute_der(&arr), arr);
        // Comparisons, logicals and negation recurse.
        let cmp = Expr::Compare {
            op: CmpOp::Gt,
            left: Box::new(der("x")),
            right: Box::new(Expr::num(0.0)),
        };
        assert_eq!(
            substitute_der(&cmp),
            Expr::Compare {
                op: CmpOp::Gt,
                left: Box::new(Expr::var("der$x")),
                right: Box::new(Expr::num(0.0)),
            }
        );
        assert_eq!(
            substitute_der(&Expr::Neg(Box::new(der("x")))),
            Expr::Neg(Box::new(Expr::var("der$x")))
        );
    }

    #[test]
    fn der_state_name_is_strict_about_its_argument() {
        assert_eq!(der_state_name(&der("x")), Some("x".into()));
        assert_eq!(der_state_name(&Expr::var("x")), None);
        assert_eq!(
            der_state_name(&Expr::call("der", vec![Expr::var("x"), Expr::var("y")])),
            None
        );
        assert_eq!(
            der_state_name(&Expr::call(
                "der",
                vec![Expr::ArrayAccess {
                    name: "t".into(),
                    indices: vec![Expr::num(1.0)]
                }]
            )),
            None
        );
    }

    #[test]
    fn collect_all_ders_walks_the_java_variants_only() {
        let mut found = Vec::new();
        let mut seen = HashSet::new();
        let e = Expr::bin(
            BinOp::Add,
            Expr::Neg(Box::new(der("a"))),
            Expr::call("max", vec![der("b"), Expr::num(1.0)]),
        );
        collect_all_ders(&e, &mut found, &mut seen);
        assert_eq!(found, ["a", "b"]);

        // An array subscript's *indices* are walked.
        let mut found = Vec::new();
        let mut seen = HashSet::new();
        collect_all_ders(
            &Expr::ArrayAccess {
                name: "t".into(),
                indices: vec![der("i")],
            },
            &mut found,
            &mut seen,
        );
        assert_eq!(found, ["i"]);

        // An array literal is a leaf, matching the Java chain's missing arm.
        let mut found = Vec::new();
        let mut seen = HashSet::new();
        collect_all_ders(&Expr::ArrayLiteral(vec![der("z")]), &mut found, &mut seen);
        assert!(found.is_empty());
    }

    #[test]
    fn java_round_rounds_halves_up_not_away_from_zero() {
        assert_eq!(java_round(2.5), 3);
        assert_eq!(java_round(2.4), 2);
        assert_eq!(java_round(-2.5), -2); // Rust's f64::round would give -3
        assert_eq!(java_round(-2.6), -3);
        assert_eq!(java_round(f64::NAN), 0);
    }

    #[test]
    fn java_max_propagates_nan() {
        assert_eq!(java_max(1.0, 2.0), 2.0);
        assert_eq!(java_max(2.0, 1.0), 2.0);
        assert!(java_max(f64::NAN, 1.0).is_nan());
        assert!(java_max(1.0, f64::NAN).is_nan());
    }

    #[test]
    fn fmt_number_drops_the_point_on_integers() {
        assert_eq!(fmt_number(0.0), "0");
        assert_eq!(fmt_number(60.0), "60");
        assert_eq!(fmt_number(-3.0), "-3");
        assert_eq!(fmt_number(1.5), "1.5");
    }

    #[test]
    fn display_undoes_the_flat_naming() {
        assert_eq!(
            display(&["m$port$t".to_string(), "w".to_string()]),
            "m.port.t, w"
        );
    }

    #[test]
    fn maximum_matching_finds_a_perfect_assignment_when_one_exists() {
        // eq0 -> {0}, eq1 -> {0, 1}
        assert_eq!(maximum_matching(&[vec![0], vec![0, 1]], 2), [true, true]);
        // Both equations can only reach column 0: one must go unmatched.
        let m = maximum_matching(&[vec![0], vec![0]], 2);
        assert_eq!(m.iter().filter(|x| **x).count(), 1);
        // No columns at all.
        assert_eq!(maximum_matching(&[vec![], vec![]], 0), [false, false]);
    }

    #[test]
    fn the_der_variable_naming_is_the_java_one() {
        assert_eq!(der_var("temp"), "der$temp");
        assert_eq!(der_var("t[3]"), "der$t[3]");
    }

    #[test]
    fn defaults_match_the_java_constants() {
        assert_eq!(DynamicOptions::DEFAULT_METHOD, "ode45");
        assert_eq!(DynamicOptions::DEFAULT_RTOL, 1e-6);
        assert_eq!(DynamicOptions::DEFAULT_ATOL, 1e-9);
        assert_eq!(DynamicOptions::DEFAULT_POINTS, 200);
        let o = DynamicOptions::defaults("t", 0.0, 10.0);
        assert_eq!(o.method, "ode45");
        assert_eq!(o.points, None);
        assert_eq!(o.step, None);
        assert_eq!(o.max_step, None);
    }
}
