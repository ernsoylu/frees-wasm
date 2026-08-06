//! A fully-assembled implicit DAE `F(t, y, y') = 0`.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/core/dae/`
//! `DaeAssembly.java` (91), `DaeResidual.java` (17) and `DaeRootFn.java` (16),
//! together with the assembly half of `ode/DynamicSolver.assembleDae` — the
//! part that is about the DAE rather than about classifying a `DYNAMIC` block.
//!
//! # Layout
//!
//! The state vector is `y = [differential states … ; algebraic auxiliaries …]`.
//! A capacitive (storage) state `X` contributes a `der(X)` term that maps to
//! `y'[state_index(X)]`; every other equation is algebraic. By the `C-R-C`
//! discipline (§2.2) the system is index-1 and square: exactly one residual per
//! unknown.
//!
//! # Division of labour with `ode/dynamic.rs`
//!
//! Classification — deciding which names are states, which are auxiliaries, and
//! rewriting `der(X)` to the reified unknown `der$X` — belongs to the `DYNAMIC`
//! block owner. This module takes the *result* of that classification (an
//! equation template plus the two name lists) and turns it into a residual,
//! a sparsity pattern, an initial `(y, y')` and the event switching functions.
//! [`assemble`] is the entry point; it is the Rust shape of `assembleDae`.

use crate::ast::{Equation, Expr};
use crate::diag::{FreesError, Result};
use crate::eval::{eval_with, EvalContext, Scope};
use std::collections::BTreeSet;

/// The prefix the `DYNAMIC` classifier reifies `der(X)` under.
///
/// Transcribed from `DynamicSolver.derVar`. `$` cannot occur in a user
/// identifier, which is what makes the reified name collision-free.
pub const DER_PREFIX: &str = "der$";

/// The reified derivative unknown for a state name.
pub fn der_var(state: &str) -> String {
    format!("{DER_PREFIX}{state}")
}

/// The residual of an implicit DAE system `F(t, y, y') = 0`.
///
/// The frees-side shape of SUNDIALS IDA's `IDAResFn`. The implementation writes
/// the residual for the current `(t, y, yp)` into `res` (length = system
/// dimension). For a frees component network this closure is assembled from the
/// expanded scalar system: algebraic connection/constitutive equations
/// contribute `lhs − rhs` (no `y'` term), while a capacitive volume's storage
/// equation contributes `y'[k] − rhs` — the `C-R-C` discipline (§2.2) keeps the
/// index at 1.
///
/// The Java signature is `void` and signals trouble by throwing, which
/// `IdaDaeSolver`'s callback turns into IDA's *recoverable* return code 1. This
/// port returns `Result` and [`crate::dae::solver`] treats `Err` the same way:
/// the step is cut and retried rather than the integration aborted.
pub trait DaeResidual {
    fn eval(&self, t: f64, y: &[f64], yp: &[f64], res: &mut [f64]) -> Result<()>;
}

/// Event (root) functions `g(t, y, y')` monitored for sign changes during a DAE
/// integration — the frees-side shape of IDA's `IDARootFn`.
///
/// These carry the §4.8 *Tier-2* structural events only: zone collapse
/// (`L_zone − ε`) and valve open/close. The high-frequency Tier-1 crossings
/// (saturation kinks, flow reversal) are **not** events — they are regularized
/// into the smooth residual/property path and integrated straight through.
pub trait DaeRootFn {
    fn eval(&self, t: f64, y: &[f64], yp: &[f64], gout: &mut [f64]) -> Result<()>;
}

/// A [`DaeResidual`] built from a Rust closure.
pub struct ClosureResidual<'a> {
    #[allow(clippy::type_complexity)]
    f: Box<dyn Fn(f64, &[f64], &[f64], &mut [f64]) -> Result<()> + 'a>,
}

impl<'a> ClosureResidual<'a> {
    pub fn new(
        f: impl Fn(f64, &[f64], &[f64], &mut [f64]) -> Result<()> + 'a,
    ) -> ClosureResidual<'a> {
        ClosureResidual { f: Box::new(f) }
    }
}

impl DaeResidual for ClosureResidual<'_> {
    fn eval(&self, t: f64, y: &[f64], yp: &[f64], res: &mut [f64]) -> Result<()> {
        (self.f)(t, y, yp, res)
    }
}

/// A [`DaeRootFn`] built from a Rust closure.
pub struct ClosureRootFn<'a> {
    #[allow(clippy::type_complexity)]
    f: Box<dyn Fn(f64, &[f64], &[f64], &mut [f64]) -> Result<()> + 'a>,
}

impl<'a> ClosureRootFn<'a> {
    pub fn new(
        f: impl Fn(f64, &[f64], &[f64], &mut [f64]) -> Result<()> + 'a,
    ) -> ClosureRootFn<'a> {
        ClosureRootFn { f: Box::new(f) }
    }
}

impl DaeRootFn for ClosureRootFn<'_> {
    fn eval(&self, t: f64, y: &[f64], yp: &[f64], gout: &mut [f64]) -> Result<()> {
        (self.f)(t, y, yp, gout)
    }
}

/// A fully-assembled implicit DAE produced from a frees expanded scalar system
/// (a classified `DYNAMIC` block) — the frees-to-IDA bridge of Phase S1.
///
/// Port of the `DaeAssembly` record. The Java record's hand-written
/// `equals`/`hashCode`/`toString` exist only because Java arrays compare by
/// identity; Rust slices already compare by value, so they have no counterpart
/// here (the closures stay non-comparable either way).
pub struct DaeAssembly<'a> {
    /// System dimension (`states + auxiliaries`).
    pub n: usize,
    /// The `y` variable names in layout order (states then aux).
    pub variables: Vec<String>,
    /// The differential-state names (prefix of [`Self::variables`]).
    pub states: Vec<String>,
    /// The algebraic-auxiliary names (suffix of [`Self::variables`]).
    pub aux: Vec<String>,
    /// Differential/algebraic marker per component: `1.0` differential, `0.0`
    /// algebraic (for `IDASetId` / `IDA_YA_YDP_INIT`).
    pub id: Vec<f64>,
    /// The residual closure.
    pub residual: Box<dyn DaeResidual + 'a>,
    /// Initial `y` (state initials; aux seeded or zero).
    pub y0: Vec<f64>,
    /// Initial `y'` guess (state derivatives seeded or zero; aux unused).
    pub yp0: Vec<f64>,
    /// Per-row column dependency lists (combined `∂F/∂y + ∂F/∂y'` pattern).
    pub sparsity: Vec<Vec<usize>>,
    /// Switching functions for §4.8 Tier-2 events, or `None`.
    pub root_fn: Option<Box<dyn DaeRootFn + 'a>>,
    /// Event names aligned with [`Self::root_fn`].
    pub event_names: Vec<String>,
    /// Whether each event halts integration.
    pub event_stops: Vec<bool>,
}

/// Mirrors the Java record's hand-written `toString`: everything but the
/// closures, which have no printable form in either language.
impl std::fmt::Debug for DaeAssembly<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaeAssembly")
            .field("n", &self.n)
            .field("variables", &self.variables)
            .field("states", &self.states)
            .field("aux", &self.aux)
            .field("id", &self.id)
            .field("y0", &self.y0)
            .field("yp0", &self.yp0)
            .field("sparsity", &self.sparsity)
            .field("has_root_fn", &self.root_fn.is_some())
            .field("event_names", &self.event_names)
            .field("event_stops", &self.event_stops)
            .finish()
    }
}

impl DaeAssembly<'_> {
    pub fn event_count(&self) -> usize {
        self.event_names.len()
    }

    /// Index of a state in the `y` layout, or `None` when the name is not a
    /// differential state.
    pub fn state_index(&self, name: &str) -> Option<usize> {
        self.states.iter().position(|s| s == name)
    }
}

/// The `name → value` map a residual or root function is evaluated against:
/// parameters, time, states (`y`), auxiliaries (`y`), and each `der$X` bound to
/// `y'[idx(X)]`.
///
/// Port of `DynamicSolver.daeValues` together with `pinTime`. The block's
/// declared time variable **and** the reserved global `time` (the name
/// component bodies use) are both pinned to the integrator's current time; the
/// alias is skipped when the document itself defines `time`, so a legacy
/// variable of that name keeps its meaning.
pub fn dae_values(
    base: &Scope,
    time_var: &str,
    states: &[String],
    aux: &[String],
    t: f64,
    y: &[f64],
    yp: &[f64],
) -> Scope {
    let mut v = base.clone();
    v.insert(time_var.to_string(), t);
    v.entry("time".to_string()).or_insert(t);
    for (k, state) in states.iter().enumerate() {
        v.insert(state.clone(), y[k]);
        v.insert(der_var(state), yp[k]);
    }
    let ns = states.len();
    for (j, name) in aux.iter().enumerate() {
        v.insert(name.clone(), y[ns + j]);
    }
    v
}

/// A residual assembled from a classified `DYNAMIC` block's equation template.
///
/// Every equation of the template becomes one residual row `lhs − rhs`,
/// evaluated in the scope [`dae_values`] builds. This is the closure the Java
/// `assembleDae` writes inline.
///
/// **Property guarding.** The Java wraps the sweep in
/// `PropertyFunctions.enterLenient()` so a stiff corrector probing states that
/// briefly leave the fluid table clamps instead of throwing (a throw becomes
/// `IDASolve -9`). This port has no lenient mode yet; a property failure
/// surfaces as `Err`, which [`crate::dae::solver`] treats as *recoverable* —
/// the step is cut and retried. That is a strictly safer default, but it is not
/// the same numerics: see the module note in `dae/mod.rs`.
pub struct EquationResidual<'a> {
    template: Vec<Equation>,
    base: Scope,
    states: Vec<String>,
    aux: Vec<String>,
    time_var: String,
    ctx: EvalContext<'a>,
}

impl<'a> EquationResidual<'a> {
    pub fn new(
        template: Vec<Equation>,
        base: Scope,
        states: Vec<String>,
        aux: Vec<String>,
        time_var: impl Into<String>,
        ctx: EvalContext<'a>,
    ) -> EquationResidual<'a> {
        EquationResidual {
            template,
            base,
            states,
            aux,
            time_var: time_var.into(),
            ctx,
        }
    }

    /// The scope this residual evaluates its equations in.
    pub fn scope_at(&self, t: f64, y: &[f64], yp: &[f64]) -> Scope {
        dae_values(
            &self.base,
            &self.time_var,
            &self.states,
            &self.aux,
            t,
            y,
            yp,
        )
    }
}

impl DaeResidual for EquationResidual<'_> {
    fn eval(&self, t: f64, y: &[f64], yp: &[f64], res: &mut [f64]) -> Result<()> {
        let v = self.scope_at(t, y, yp);
        for (i, eq) in self.template.iter().enumerate() {
            res[i] = eval_with(&eq.lhs, &v, self.ctx)? - eval_with(&eq.rhs, &v, self.ctx)?;
        }
        Ok(())
    }
}

/// Root functions assembled from a classified block's event list: `g_r = lhs_r
/// − rhs_r`, evaluated in the same scope as the residual.
///
/// Port of `DynamicSolver.buildRootFn`.
pub struct EquationRootFn<'a> {
    lhs: Vec<Expr>,
    rhs: Vec<Expr>,
    base: Scope,
    states: Vec<String>,
    aux: Vec<String>,
    time_var: String,
    ctx: EvalContext<'a>,
}

impl<'a> EquationRootFn<'a> {
    pub fn new(
        lhs: Vec<Expr>,
        rhs: Vec<Expr>,
        base: Scope,
        states: Vec<String>,
        aux: Vec<String>,
        time_var: impl Into<String>,
        ctx: EvalContext<'a>,
    ) -> EquationRootFn<'a> {
        EquationRootFn {
            lhs,
            rhs,
            base,
            states,
            aux,
            time_var: time_var.into(),
            ctx,
        }
    }
}

impl DaeRootFn for EquationRootFn<'_> {
    fn eval(&self, t: f64, y: &[f64], yp: &[f64], gout: &mut [f64]) -> Result<()> {
        let v = dae_values(
            &self.base,
            &self.time_var,
            &self.states,
            &self.aux,
            t,
            y,
            yp,
        );
        for (r, slot) in gout.iter_mut().enumerate().take(self.lhs.len()) {
            *slot = eval_with(&self.lhs[r], &v, self.ctx)? - eval_with(&self.rhs[r], &v, self.ctx)?;
        }
        Ok(())
    }
}

/// Per-row column dependency lists: a variable hits its own column; a `der$X`
/// reference hits state `X`'s column (the `∂F/∂y'` term shares the column with
/// `∂F/∂y` in IDA's combined system matrix).
///
/// Port of `DynamicSolver.buildSparsity` + `addColumns`. Columns come out
/// ascending (the Java uses a `TreeSet`), which the CSC assembly relies on.
pub fn build_sparsity(
    template: &[Equation],
    column: &std::collections::HashMap<String, usize>,
    ns: usize,
) -> Vec<Vec<usize>> {
    template
        .iter()
        .map(|eq| {
            let mut cols: BTreeSet<usize> = BTreeSet::new();
            add_columns(&eq.lhs, column, ns, &mut cols);
            add_columns(&eq.rhs, column, ns, &mut cols);
            cols.into_iter().collect()
        })
        .collect()
}

fn add_columns(
    e: &Expr,
    column: &std::collections::HashMap<String, usize>,
    ns: usize,
    cols: &mut BTreeSet<usize>,
) {
    for var in e.variables() {
        if let Some(&col) = column.get(&var) {
            cols.insert(col);
        } else if let Some(state) = var.strip_prefix(DER_PREFIX) {
            if let Some(&sc) = column.get(state) {
                if sc < ns {
                    cols.insert(sc);
                }
            }
        }
    }
}

/// An event as the assembler needs it: the two sides of its switching
/// expression (already `der`-substituted), its name and whether it stops the
/// integration.
pub struct EventSpec {
    pub name: String,
    pub lhs: Expr,
    pub rhs: Expr,
    pub stops: bool,
}

/// Everything [`assemble`] needs from a classified `DYNAMIC` block.
pub struct AssemblySpec<'a> {
    /// Block name, for diagnostics.
    pub block_name: String,
    /// The block's declared time variable.
    pub time_var: String,
    /// Differential state names, in `der()` declaration order.
    pub states: Vec<String>,
    /// Algebraic auxiliary names.
    pub aux: Vec<String>,
    /// The combined algebraic block: one equation per unknown, `der(X)` already
    /// reified to `der$X`.
    pub template: Vec<Equation>,
    /// Values pinned from the analytic solve (parameters, constants).
    pub analytic_values: Scope,
    /// State initial values, aligned with [`Self::states`].
    pub state_initials: Vec<f64>,
    /// Result of one inner algebraic solve at `t0`, used to seed the auxiliary
    /// values and the state derivatives. `None` leaves zeros for the consistent
    /// initialization to resolve, which is what the Java does when its seeding
    /// solve throws.
    pub seed: Option<Scope>,
    /// Tier-2 structural events.
    pub events: Vec<EventSpec>,
    /// Document definitions for the evaluator.
    pub ctx: EvalContext<'a>,
}

/// Assembles a classified `DYNAMIC` block into an implicit DAE `F(t,y,y')=0`.
///
/// Port of `DynamicSolver.assembleDae`. The state vector is
/// `y = [states ; auxiliaries]`; each reified `der$X` maps to
/// `y'[state_index(X)]`, so every equation of the template becomes one residual
/// and the system is square and index-1. Auxiliary and derivative initial
/// guesses are seeded from [`AssemblySpec::seed`] where possible (the
/// consistent initialization then makes them consistent); its absence leaves
/// zeros.
///
/// # Diagnostic gap, deliberately
///
/// A non-square template is rejected with the Java's own wording *minus* its
/// trailing `Blocker.diagnose(probe)` sentence, which names the exact
/// unmatched equation/variable pair. That probe needs the block's states and
/// time pinned as pseudo-equations and belongs to the `DYNAMIC` owner, who has
/// the blocker in hand; it should append the sentence to this message.
pub fn assemble<'a>(spec: AssemblySpec<'a>) -> Result<DaeAssembly<'a>> {
    let ns = spec.states.len();
    let n = ns + spec.aux.len();
    if spec.template.len() != n {
        return Err(FreesError::solver(non_square_diagnostic(&spec, n)));
    }
    // The Java builds `y0` inside the classifier so this cannot mismatch there;
    // here the spec is caller-built, so a mismatch is diagnosed rather than
    // becoming a slice-length panic.
    if spec.state_initials.len() != ns {
        return Err(FreesError::solver(format!(
            "DYNAMIC {}: {} initial value{} for {ns} state{}.",
            spec.block_name,
            spec.state_initials.len(),
            if spec.state_initials.len() == 1 {
                ""
            } else {
                "s"
            },
            if ns == 1 { "" } else { "s" }
        )));
    }

    let mut variables = spec.states.clone();
    variables.extend(spec.aux.iter().cloned());
    let column: std::collections::HashMap<String, usize> = variables
        .iter()
        .enumerate()
        .map(|(k, v)| (v.clone(), k))
        .collect();

    let mut id = vec![0.0; n];
    for slot in id.iter_mut().take(ns) {
        *slot = 1.0; // differential
    }

    let sparsity = build_sparsity(&spec.template, &column, ns);

    let mut y0 = vec![0.0; n];
    let mut yp0 = vec![0.0; n];
    y0[..ns].copy_from_slice(&spec.state_initials);
    if let Some(seed) = &spec.seed {
        for (k, state) in spec.states.iter().enumerate() {
            yp0[k] = seed.get(&der_var(state)).copied().unwrap_or(0.0);
        }
        for (j, name) in spec.aux.iter().enumerate() {
            y0[ns + j] = seed.get(name).copied().unwrap_or(0.0);
        }
    }

    let event_names: Vec<String> = spec.events.iter().map(|e| e.name.clone()).collect();
    let event_stops: Vec<bool> = spec.events.iter().map(|e| e.stops).collect();
    let root_fn: Option<Box<dyn DaeRootFn + 'a>> = if spec.events.is_empty() {
        None
    } else {
        Some(Box::new(EquationRootFn::new(
            spec.events.iter().map(|e| e.lhs.clone()).collect(),
            spec.events.iter().map(|e| e.rhs.clone()).collect(),
            spec.analytic_values.clone(),
            spec.states.clone(),
            spec.aux.clone(),
            spec.time_var.clone(),
            spec.ctx,
        )))
    };

    let residual = Box::new(EquationResidual::new(
        spec.template,
        spec.analytic_values,
        spec.states.clone(),
        spec.aux.clone(),
        spec.time_var,
        spec.ctx,
    ));

    Ok(DaeAssembly {
        n,
        variables,
        states: spec.states,
        aux: spec.aux,
        id,
        residual,
        y0,
        yp0,
        sparsity,
        root_fn,
        event_names,
        event_stops,
    })
}

/// A non-square DAE assembly explained in the model's own vocabulary: which
/// variables the network carries, how many equations it produced, and the usual
/// physical cause. Display names, never flat internals.
///
/// Port of `DynamicSolver.nonSquareDiagnostic`.
fn non_square_diagnostic(spec: &AssemblySpec<'_>, n: usize) -> String {
    let m = spec.template.len();
    let mut s = format!(
        "DYNAMIC {}: the network's equation set is {} ({} equations for {} unknowns: {} state{} + {} algebraic).",
        spec.block_name,
        if m < n { "underdetermined" } else { "overdetermined" },
        m,
        n,
        spec.states.len(),
        if spec.states.len() == 1 { "" } else { "s" },
        spec.aux.len()
    );
    if m < n {
        s.push_str(
            " A common cause: a branch has no flow-determining element — an \
             efficiency-only machine or rigid pass-through feeding a storage \
             volume leaves the through-flow free; add an orifice/valve/flow \
             map, or pin a boundary flow.",
        );
    } else {
        s.push_str(
            " A common cause: a boundary pins a quantity a component already \
             defines (e.g. re-equating a mixer pressure or T-pinning a wall \
             state).",
        );
    }
    s.push_str(&format!(" States: {}.", display(&spec.states)));
    s
}

/// Flat solver names → dotted display names.
fn display(names: &[String]) -> String {
    names
        .iter()
        .map(|v| v.replace('$', "."))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinOp;
    use std::collections::HashMap;

    fn scope(pairs: &[(&str, f64)]) -> Scope {
        pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
    }

    /// A one-state, one-aux Newton-cooling network:
    ///   der$temp = q / cap
    ///   q        = k * (tinf - temp)
    fn cooling_spec<'a>() -> AssemblySpec<'a> {
        AssemblySpec {
            block_name: "cool".into(),
            time_var: "time".into(),
            states: vec!["temp".into()],
            aux: vec!["q".into()],
            template: vec![
                Equation::new(
                    Expr::var("der$temp"),
                    Expr::bin(BinOp::Div, Expr::var("q"), Expr::var("cap")),
                    "der(Temp) = q/cap",
                ),
                Equation::new(
                    Expr::var("q"),
                    Expr::bin(
                        BinOp::Mul,
                        Expr::var("k"),
                        Expr::bin(BinOp::Sub, Expr::var("tinf"), Expr::var("temp")),
                    ),
                    "q = k*(Tinf - Temp)",
                ),
            ],
            analytic_values: scope(&[("cap", 2.0), ("k", 0.1), ("tinf", 20.0)]),
            state_initials: vec![95.0],
            seed: None,
            events: Vec::new(),
            ctx: EvalContext::default(),
        }
    }

    #[test]
    fn layout_is_states_then_aux_with_matching_id() {
        let dae = assemble(cooling_spec()).unwrap();
        assert_eq!(dae.n, 2);
        assert_eq!(dae.variables, vec!["temp".to_string(), "q".to_string()]);
        assert_eq!(dae.id, vec![1.0, 0.0]);
        assert_eq!(dae.y0, vec![95.0, 0.0]);
        assert_eq!(dae.yp0, vec![0.0, 0.0]);
        assert_eq!(dae.state_index("temp"), Some(0));
        assert_eq!(dae.state_index("q"), None);
    }

    #[test]
    fn residual_binds_der_to_yp_and_aux_to_y() {
        let dae = assemble(cooling_spec()).unwrap();
        let mut res = vec![0.0; 2];
        // At temp=95, q=7.5, yp=3.75: row 0 = 3.75 - 7.5/2 = 0; row 1 = 7.5 - 0.1*(20-95) = 15.
        dae.residual
            .eval(0.0, &[95.0, 7.5], &[3.75, 0.0], &mut res)
            .unwrap();
        assert_eq!(res[0], 0.0);
        assert_eq!(res[1], 7.5 - 0.1 * (20.0 - 95.0));
    }

    #[test]
    fn sparsity_folds_der_onto_its_state_column() {
        let dae = assemble(cooling_spec()).unwrap();
        // Row 0 mentions der$temp (-> column 0) and q (column 1). `cap` is a
        // parameter, not a column.
        assert_eq!(dae.sparsity[0], vec![0, 1]);
        // Row 1 mentions q (1) and temp (0).
        assert_eq!(dae.sparsity[1], vec![0, 1]);
    }

    #[test]
    fn seeding_fills_aux_values_and_state_derivatives() {
        let mut spec = cooling_spec();
        spec.seed = Some(scope(&[("q", -7.5), ("der$temp", -3.75)]));
        let dae = assemble(spec).unwrap();
        assert_eq!(dae.y0, vec![95.0, -7.5]);
        assert_eq!(dae.yp0, vec![-3.75, 0.0]);
    }

    #[test]
    fn a_non_square_template_is_rejected_in_the_model_vocabulary() {
        let mut spec = cooling_spec();
        spec.template.pop();
        let err = assemble(spec).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("underdetermined"), "{msg}");
        assert!(msg.contains("1 equations for 2 unknowns"), "{msg}");
        assert!(msg.contains("1 state + 1 algebraic"), "{msg}");
        assert!(msg.contains("States: temp"), "{msg}");
    }

    #[test]
    fn an_overdetermined_template_names_the_other_cause() {
        let mut spec = cooling_spec();
        let extra = spec.template[1].clone();
        spec.template.push(extra);
        let err = assemble(spec).unwrap_err().to_string();
        assert!(err.contains("overdetermined"), "{err}");
        assert!(err.contains("a boundary pins a quantity"), "{err}");
    }

    #[test]
    fn time_alias_does_not_shadow_a_document_variable_named_time() {
        // `analyticValues` already carries `time`; pinTime must leave it alone
        // and pin only the block's declared time variable.
        let base = scope(&[("time", 42.0)]);
        let v = dae_values(&base, "tau", &["x".to_string()], &[], 7.0, &[1.0], &[2.0]);
        assert_eq!(v["tau"], 7.0);
        assert_eq!(v["time"], 42.0, "a document `time` keeps its meaning");
        assert_eq!(v["x"], 1.0);
        assert_eq!(v["der$x"], 2.0);
    }

    #[test]
    fn time_alias_is_pinned_when_the_document_does_not_define_it() {
        let v = dae_values(&Scope::default(), "tau", &[], &[], 7.0, &[], &[]);
        assert_eq!(v["tau"], 7.0);
        assert_eq!(v["time"], 7.0);
    }

    #[test]
    fn events_become_root_functions_named_and_flagged() {
        let mut spec = cooling_spec();
        spec.events = vec![EventSpec {
            name: "cold".into(),
            lhs: Expr::var("temp"),
            rhs: Expr::num(30.0),
            stops: true,
        }];
        let dae = assemble(spec).unwrap();
        assert_eq!(dae.event_count(), 1);
        assert_eq!(dae.event_names, vec!["cold".to_string()]);
        assert_eq!(dae.event_stops, vec![true]);
        let mut g = vec![0.0];
        dae.root_fn
            .as_ref()
            .unwrap()
            .eval(0.0, &[95.0, 0.0], &[0.0, 0.0], &mut g)
            .unwrap();
        assert_eq!(g[0], 65.0);
    }

    #[test]
    fn no_events_means_no_root_function() {
        let dae = assemble(cooling_spec()).unwrap();
        assert!(dae.root_fn.is_none());
        assert_eq!(dae.event_count(), 0);
    }

    #[test]
    fn a_state_initials_length_mismatch_is_diagnosed_not_a_panic() {
        let mut spec = cooling_spec();
        spec.state_initials = vec![95.0, 1.0];
        let err = assemble(spec).unwrap_err().to_string();
        assert!(err.contains("2 initial values for 1 state"), "{err}");
    }

    #[test]
    fn build_sparsity_ignores_a_der_of_an_auxiliary() {
        // der$q where q is an auxiliary (column 1, not < ns) must not add a
        // column — the Java guards on `sc < ns` for exactly this.
        let column: HashMap<String, usize> = [("x".to_string(), 0), ("q".to_string(), 1)]
            .into_iter()
            .collect();
        let template = vec![Equation::new(
            Expr::var("der$q"),
            Expr::num(0.0),
            "der$q = 0",
        )];
        assert_eq!(
            build_sparsity(&template, &column, 1),
            vec![Vec::<usize>::new()]
        );
    }
}
