//! The ODE Table accessors — how the *analytic* system reads a solved
//! `DYNAMIC` block.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/core/ode/OdeAccessors.java`
//! (160 LOC) and `DynamicAccessorContext.java` (155 LOC).
//!
//! | Accessor | Meaning |
//! |---|---|
//! | `FinalValue('col')` | last sampled value |
//! | `MaxValue('col')` / `MinValue('col')` | extrema (also `ODEMax` / `ODEMin`) |
//! | `ODEValue('col', t)` | value at time `t` (linearly interpolated) |
//! | `TimeAt('col', v)` | first time the column crosses `v` |
//! | `ODEAvg` / `ODESum` / `ODEStdDev('col')` | column aggregates |
//!
//! These are **live** during the analytic solve: each evaluates against an ODE
//! table integrated with the current Newton iterate, so the analytic solver can
//! size an ODE input to hit a transient target (e.g. apogee altitude = 100 km).
//! That is the second-solve pass the module docs of [`crate::ode::dynamic`]
//! describe from the other side.
//!
//! # An explicit context, not a thread-local
//!
//! The Java hangs the live bridge on a `ThreadLocal` and has `Evaluator` reach
//! for it statically. This port passes [`DynamicAccessorContext`] explicitly and
//! reproduces the two behaviours the thread-local encoded:
//!
//! * **No context installed → `0.0`.** [`resolve`] takes an `Option` and returns
//!   zero for `None`, exactly as `DynamicAccessorContext.resolve` does when
//!   `CURRENT.get()` is null. That is what makes an accessor harmless in a
//!   document with no `DYNAMIC` block.
//! * **No re-entry while integrating.** The Java removes the context for the
//!   duration of `runner.run(...)` so the block's own algebraic solves cannot
//!   resolve accessors recursively. Here a [`Cell`] flag does it, and it works
//!   even though the context is reachable through the call chain.
//!
//! Everything else — the per-block cache keyed on the block's input-variable
//! signature, the display→flat name mapping, array-element column ownership — is
//! transcribed.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};

use crate::ast::{Equation, Expr};
use crate::diag::{FreesError, Result};
use crate::eval::Scope;
use crate::ode::analysis::{analyze, Shape};
use crate::ode::dynamic::DynamicSystem;
use crate::ode::problem::OdeTableResult;

/// Every accessor name, lowercase. Port of `OdeAccessors.NAMES`.
pub const NAMES: [&str; 10] = [
    "odevalue",
    "finalvalue",
    "maxvalue",
    "minvalue",
    "timeat",
    "odeavg",
    "odesum",
    "odestddev",
    "odemin",
    "odemax",
];

/// Whether `function` names an ODE Table accessor (case-insensitive).
pub fn is_accessor(function: &str) -> bool {
    let lower = function.to_ascii_lowercase();
    NAMES.contains(&lower.as_str())
}

/// Whether any equation references an ODE accessor function.
///
/// Port of `OdeAccessors.containsAccessor`. The engine uses this to decide
/// whether the analytic solve needs the live bridge installed at all.
pub fn contains_accessor(equations: &[Equation]) -> bool {
    equations
        .iter()
        .any(|eq| expr_has_accessor(&eq.lhs) || expr_has_accessor(&eq.rhs))
}

fn expr_has_accessor(e: &Expr) -> bool {
    match e {
        Expr::Call { function, args } => {
            is_accessor(function) || args.iter().any(expr_has_accessor)
        }
        Expr::BinOp { left, right, .. }
        | Expr::Compare { left, right, .. }
        | Expr::Logical { left, right, .. } => expr_has_accessor(left) || expr_has_accessor(right),
        Expr::Neg(operand) | Expr::Not(operand) => expr_has_accessor(operand),
        _ => false,
    }
}

/// The column an accessor call names, if its first argument is a string literal
/// or a bare identifier.
///
/// Port of `EquationSystemSolver.collectAccessorColumns`'s inner test, kept here
/// with the rest of the accessor vocabulary so the engine does not have to
/// re-derive what "the column argument" means.
pub fn accessor_column(function: &str, args: &[Expr]) -> Option<String> {
    if !is_accessor(function) || args.is_empty() {
        return None;
    }
    match &args[0] {
        Expr::Str(s) => Some(s.to_ascii_lowercase()),
        Expr::Var(n) => Some(n.to_ascii_lowercase()),
        _ => None,
    }
}

/// Every column named by an accessor anywhere in `e`, in first-seen order.
///
/// Port of `EquationSystemSolver.collectAccessorColumns`.
pub fn collect_accessor_columns(e: &Expr, cols: &mut Vec<String>) {
    match e {
        Expr::Call { function, args } => {
            if let Some(col) = accessor_column(function, args) {
                if !cols.contains(&col) {
                    cols.push(col);
                }
            }
            for a in args {
                collect_accessor_columns(a, cols);
            }
        }
        Expr::BinOp { left, right, .. }
        | Expr::Compare { left, right, .. }
        | Expr::Logical { left, right, .. } => {
            collect_accessor_columns(left, cols);
            collect_accessor_columns(right, cols);
        }
        Expr::Neg(operand) | Expr::Not(operand) => collect_accessor_columns(operand, cols),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Computing one accessor over a solved table
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Stat {
    Max,
    Min,
    Avg,
    Sum,
    Std,
}

/// Computes one accessor over a solved ODE table.
///
/// Port of `OdeAccessors.compute`.
pub fn compute(
    table: &OdeTableResult,
    function: &str,
    column: &str,
    arg: Option<f64>,
) -> Result<f64> {
    let wanted = column.to_ascii_lowercase();
    let Some(ci) = table.columns.iter().position(|c| *c == wanted) else {
        return Err(FreesError::solver(format!(
            "ODE accessor: column '{column}' not found in DYNAMIC '{}'. Available columns: [{}].",
            table.name,
            table.columns.join(", ")
        )));
    };
    if table.rows.is_empty() {
        return Err(FreesError::solver(format!(
            "ODE accessor: DYNAMIC '{}' produced no rows.",
            table.name
        )));
    }
    let rows = &table.rows;
    match function.to_ascii_lowercase().as_str() {
        "finalvalue" => Ok(rows[rows.len() - 1][ci]),
        "maxvalue" | "odemax" => Ok(stat(rows, ci, Stat::Max)),
        "minvalue" | "odemin" => Ok(stat(rows, ci, Stat::Min)),
        "odeavg" => Ok(stat(rows, ci, Stat::Avg)),
        "odesum" => Ok(stat(rows, ci, Stat::Sum)),
        "odestddev" => Ok(stat(rows, ci, Stat::Std)),
        "odevalue" => Ok(value_at_time(table, ci, require_arg(arg, "ODEValue")?)),
        "timeat" => time_at_crossing(table, ci, require_arg(arg, "TimeAt")?),
        other => Err(FreesError::solver(format!(
            "Unknown ODE accessor '{other}'."
        ))),
    }
}

fn stat(rows: &[Vec<f64>], ci: usize, kind: Stat) -> f64 {
    let mut sum = 0.0;
    let mut max = f64::NEG_INFINITY;
    let mut min = f64::INFINITY;
    let n = rows.len();
    for row in rows {
        let v = row[ci];
        sum += v;
        // `Math.max`/`Math.min` propagate NaN where Rust's `f64::max`/`f64::min`
        // return the other operand; a NaN cell must poison the extremum here as
        // it does in the Java.
        max = if max.is_nan() || v.is_nan() {
            f64::NAN
        } else if v > max {
            v
        } else {
            max
        };
        min = if min.is_nan() || v.is_nan() {
            f64::NAN
        } else if v < min {
            v
        } else {
            min
        };
    }
    let mean = sum / n as f64;
    match kind {
        Stat::Max => max,
        Stat::Min => min,
        Stat::Sum => sum,
        Stat::Avg => mean,
        Stat::Std => {
            let mut sq = 0.0;
            for row in rows {
                let d = row[ci] - mean;
                sq += d * d;
            }
            // Population standard deviation (divide by n), as the Java writes it.
            (sq / n as f64).sqrt()
        }
    }
}

/// Linear interpolation of column `ci` at the given time (column 0), clamped to
/// the nearest end outside the sampled window.
fn value_at_time(table: &OdeTableResult, ci: usize, time: f64) -> f64 {
    let rows = &table.rows;
    for i in 0..rows.len().saturating_sub(1) {
        let t0 = rows[i][0];
        let t1 = rows[i + 1][0];
        if (time >= t0 && time <= t1) || (time <= t0 && time >= t1) {
            let f = if t1 == t0 {
                0.0
            } else {
                (time - t0) / (t1 - t0)
            };
            return rows[i][ci] + f * (rows[i + 1][ci] - rows[i][ci]);
        }
    }
    // Clamp to the nearest end.
    if time <= rows[0][0] {
        rows[0][ci]
    } else {
        rows[rows.len() - 1][ci]
    }
}

/// First time at which column `ci` crosses `target` (interpolated).
fn time_at_crossing(table: &OdeTableResult, ci: usize, target: f64) -> Result<f64> {
    let rows = &table.rows;
    for i in 0..rows.len().saturating_sub(1) {
        let a = rows[i][ci] - target;
        let b = rows[i + 1][ci] - target;
        if a == 0.0 {
            return Ok(rows[i][0]);
        }
        if (a < 0.0) != (b < 0.0) {
            let f = a / (a - b);
            let t0 = rows[i][0];
            let t1 = rows[i + 1][0];
            return Ok(t0 + f * (t1 - t0));
        }
    }
    Err(FreesError::solver(format!(
        "TimeAt: column never reaches {target} in DYNAMIC '{}'.",
        table.name
    )))
}

fn require_arg(arg: Option<f64>, function: &str) -> Result<f64> {
    arg.ok_or_else(|| {
        FreesError::solver(format!(
            "{function} requires a second argument, e.g. {function}('col', value)."
        ))
    })
}

// ---------------------------------------------------------------------------
// The live bridge
// ---------------------------------------------------------------------------

/// Integrates one block against a value map, producing its ODE table.
///
/// Port of `DynamicAccessorContext.BlockRunner`. The engine supplies a closure
/// that builds a [`crate::ode::dynamic::DynamicSolver`] over the current Newton
/// iterate and solves it.
pub trait BlockRunner {
    fn run(&mut self, system: &DynamicSystem, values: &Scope) -> Result<OdeTableResult>;
}

impl<F> BlockRunner for F
where
    F: FnMut(&DynamicSystem, &Scope) -> Result<OdeTableResult>,
{
    fn run(&mut self, system: &DynamicSystem, values: &Scope) -> Result<OdeTableResult> {
        self(system, values)
    }
}

struct CacheEntry {
    signature: Vec<Option<f64>>,
    table: OdeTableResult,
}

/// The bridge that makes ODE Table accessors live during the analytic solve.
///
/// Port of `DynamicAccessorContext`. When the evaluator meets `MaxValue('h')` it
/// calls [`resolve`], which finds the `DYNAMIC` block owning that column,
/// integrates it with the current Newton iterate, and computes the requested
/// statistic.
///
/// Results are cached per block, keyed on the block's input-variable signature,
/// so several accessors at the same Newton point reuse one integration. A
/// signature containing a `NaN` never matches itself — that is a cache miss, not
/// a wrong answer, and it is the only place this diverges from the Java
/// `List<Double>.equals`, which treats `NaN` as equal to itself.
pub struct DynamicAccessorContext<'a> {
    systems: &'a [DynamicSystem],
    shapes: Vec<Shape>,
    /// display→flat name map (both lowercased), so an accessor can address a
    /// component's transient state by its dotted display name (`m.port.t`) while
    /// the table and shape use flat names (`m$port$t`).
    display_to_flat: BTreeMap<String, String>,
    runner: RefCell<Box<dyn BlockRunner + 'a>>,
    cache: RefCell<HashMap<usize, CacheEntry>>,
    /// Set while a block is being integrated, so the block's own algebraic
    /// solves cannot re-enter accessor resolution (the Java removes the
    /// thread-local for the same reason).
    integrating: Cell<bool>,
}

impl<'a> DynamicAccessorContext<'a> {
    /// Build a context over the document's blocks. Port of
    /// `DynamicAccessorContext.install` (minus the thread-local store).
    pub fn install(
        systems: &'a [DynamicSystem],
        display_to_flat: BTreeMap<String, String>,
        runner: Box<dyn BlockRunner + 'a>,
    ) -> DynamicAccessorContext<'a> {
        let shapes = systems.iter().map(analyze).collect();
        DynamicAccessorContext {
            systems,
            shapes,
            display_to_flat,
            runner: RefCell::new(runner),
            cache: RefCell::new(HashMap::new()),
            integrating: Cell::new(false),
        }
    }

    /// Resolve one accessor against the block that owns its column.
    ///
    /// Port of `DynamicAccessorContext.doResolve`, plus the re-entrancy guard
    /// the Java gets by unsetting its thread-local: a nested call while a block
    /// is integrating yields `0.0`, the same value the Java's null-context path
    /// returns.
    pub fn resolve(
        &self,
        function: &str,
        column: &str,
        arg: Option<f64>,
        values: &Scope,
    ) -> Result<f64> {
        if self.integrating.get() {
            return Ok(0.0);
        }
        // Accept a component state's dotted display name (m.port.t) as well as
        // its flat name (m$port$t) — the table/shape are keyed on the flat name.
        // The Java default is the *original* spelling, not the lowercased probe.
        let resolved = self
            .display_to_flat
            .get(&column.to_ascii_lowercase())
            .cloned()
            .unwrap_or_else(|| column.to_string());
        let Some(block) = self.owner_block(&resolved) else {
            return Err(FreesError::solver(format!(
                "ODE accessor: no DYNAMIC block has a column '{column}'. Check the column name."
            )));
        };
        let signature = self.signature_of(block, values);
        if let Some(cached) = self.cache.borrow().get(&block) {
            if cached.signature == signature {
                return compute(&cached.table, function, &resolved, arg);
            }
        }
        let table = self.integrate(block, values)?;
        let out = compute(&table, function, &resolved, arg);
        self.cache
            .borrow_mut()
            .insert(block, CacheEntry { signature, table });
        out
    }

    /// Run one block with re-entrancy disabled for the duration, restoring the
    /// flag on every exit path (the Java's `try`/`finally`).
    fn integrate(&self, block: usize, values: &Scope) -> Result<OdeTableResult> {
        self.integrating.set(true);
        let outcome = self.runner.borrow_mut().run(&self.systems[block], values);
        self.integrating.set(false);
        outcome
    }

    /// The index of the block owning `column`, or `None`.
    fn owner_block(&self, column: &str) -> Option<usize> {
        let col = column.to_ascii_lowercase();
        self.shapes
            .iter()
            .position(|shape| owns_column(shape, &col))
    }

    /// The input-variable signature of a block at the current iterate. A missing
    /// value stays `None`, exactly as the Java stores a `null`.
    fn signature_of(&self, block: usize, values: &Scope) -> Vec<Option<f64>> {
        self.shapes[block]
            .input_vars
            .iter()
            .map(|v| values.get(v).copied())
            .collect()
    }

    /// The set of input variables of the block owning `column`; empty if no
    /// block owns it. Port of `DynamicAccessorContext.inputVarsForColumn`, using
    /// the shapes this context already analysed.
    pub fn input_vars_for_column(&self, column: &str) -> Vec<String> {
        match self.owner_block(column) {
            Some(block) => self.shapes[block].input_vars.iter().cloned().collect(),
            None => Vec::new(),
        }
    }
}

/// [`DynamicAccessorContext::resolve`] with the Java's null-context behaviour:
/// no context installed means `0.0`, never an error.
///
/// Port of the static `DynamicAccessorContext.resolve`.
pub fn resolve(
    ctx: Option<&DynamicAccessorContext<'_>>,
    function: &str,
    column: &str,
    arg: Option<f64>,
    values: &Scope,
) -> Result<f64> {
    match ctx {
        None => Ok(0.0),
        Some(ctx) => ctx.resolve(function, column, arg, values),
    }
}

/// The set of input variables of the block owning `column`, analysing the
/// systems from scratch; empty if no block owns it.
///
/// Port of the static `DynamicAccessorContext.inputVarsForColumn`, which the
/// engine calls before any context exists (while augmenting the accessor
/// equations' structural dependencies).
pub fn input_vars_for_column(systems: &[DynamicSystem], column: &str) -> Vec<String> {
    let col = column.to_ascii_lowercase();
    for ds in systems {
        let shape = analyze(ds);
        if owns_column(&shape, &col) {
            return shape.input_vars.into_iter().collect();
        }
    }
    Vec::new()
}

/// Whether a block owns a column.
///
/// Port of `DynamicAccessorContext.ownsColumn`. Array-element columns (`t[4]`)
/// match by their base state/aux name (`t`), since the static analysis does not
/// expand the `FOR`/array discretization — see [`crate::ode::analysis`].
pub fn owns_column(shape: &Shape, col: &str) -> bool {
    if shape.columns.iter().any(|c| c == col) {
        return true;
    }
    match col.find('[') {
        Some(bracket) if bracket > 0 => {
            let base = &col[..bracket];
            shape.states.iter().any(|s| s == base) || shape.aux.iter().any(|a| a == base)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinOp;
    use crate::ode::dynamic::{DynamicOptions, InitialCondition};
    use crate::ode::problem::TableEventHit;

    fn table(columns: &[&str], rows: Vec<Vec<f64>>) -> OdeTableResult {
        OdeTableResult {
            name: "cool".into(),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            rows,
            events: Vec::new(),
            method: "ode45".into(),
            stopped: false,
            end_time: 60.0,
        }
    }

    /// The oracle-verified Newton-cooling table (k = 0.05, Tinf = 20,
    /// Temp(0) = 95, ode45, 0..60, 4 points).
    fn cooling_table() -> OdeTableResult {
        table(
            &["time", "temp"],
            vec![
                vec![0.0, 95.0],
                vec![20.0, 47.59095803046333],
                vec![40.0, 30.15014623853744],
                vec![60.0, 23.734030127668667],
            ],
        )
    }

    // -- name vocabulary -----------------------------------------------------

    #[test]
    fn every_accessor_name_is_recognised_case_insensitively() {
        for name in NAMES {
            assert!(is_accessor(name), "{name}");
            assert!(is_accessor(&name.to_ascii_uppercase()), "{name}");
        }
        assert!(is_accessor("FinalValue"));
        assert!(is_accessor("ODEStdDev"));
        assert!(!is_accessor("finalvalue2"));
        assert!(!is_accessor("sum"));
        assert_eq!(NAMES.len(), 10);
    }

    #[test]
    fn contains_accessor_walks_both_sides_and_nests() {
        let plain = Equation::new(Expr::var("a"), Expr::var("b"), "a = b");
        assert!(!contains_accessor(std::slice::from_ref(&plain)));

        let nested = Equation::new(
            Expr::var("h"),
            Expr::bin(
                BinOp::Mul,
                Expr::num(2.0),
                Expr::call("maxvalue", vec![Expr::Str("temp".into())]),
            ),
            "h = 2*MaxValue('temp')",
        );
        assert!(contains_accessor(&[plain.clone(), nested]));

        let on_the_left = Equation::new(
            Expr::Neg(Box::new(Expr::call(
                "finalvalue",
                vec![Expr::Str("temp".into())],
            ))),
            Expr::num(0.0),
            "-FinalValue('temp') = 0",
        );
        assert!(contains_accessor(&[on_the_left]));
    }

    #[test]
    fn accessor_columns_come_from_string_or_identifier_arguments() {
        assert_eq!(
            accessor_column("MaxValue", &[Expr::Str("Temp".into())]),
            Some("temp".into())
        );
        assert_eq!(
            accessor_column("timeat", &[Expr::var("Temp"), Expr::num(50.0)]),
            Some("temp".into())
        );
        assert_eq!(accessor_column("maxvalue", &[]), None);
        assert_eq!(accessor_column("sin", &[Expr::Str("x".into())]), None);
        assert_eq!(accessor_column("maxvalue", &[Expr::num(1.0)]), None);

        let mut cols = Vec::new();
        collect_accessor_columns(
            &Expr::bin(
                BinOp::Add,
                Expr::call("maxvalue", vec![Expr::Str("temp".into())]),
                Expr::call("finalvalue", vec![Expr::Str("temp".into())]),
            ),
            &mut cols,
        );
        assert_eq!(cols, ["temp"]); // de-duplicated
    }

    // -- compute -------------------------------------------------------------

    #[test]
    fn final_max_and_min_read_the_column() {
        let t = cooling_table();
        assert_eq!(
            compute(&t, "FinalValue", "temp", None).unwrap(),
            23.734030127668667
        );
        assert_eq!(compute(&t, "MaxValue", "temp", None).unwrap(), 95.0);
        assert_eq!(compute(&t, "odemax", "TEMP", None).unwrap(), 95.0);
        assert_eq!(
            compute(&t, "MinValue", "temp", None).unwrap(),
            23.734030127668667
        );
        assert_eq!(
            compute(&t, "odemin", "temp", None).unwrap(),
            23.734030127668667
        );
        // The time column is a column like any other.
        assert_eq!(compute(&t, "FinalValue", "time", None).unwrap(), 60.0);
    }

    #[test]
    fn the_aggregates_are_sum_mean_and_population_stddev() {
        let t = table(
            &["time", "x"],
            vec![
                vec![0.0, 1.0],
                vec![1.0, 2.0],
                vec![2.0, 3.0],
                vec![3.0, 4.0],
            ],
        );
        assert_eq!(compute(&t, "ODESum", "x", None).unwrap(), 10.0);
        assert_eq!(compute(&t, "ODEAvg", "x", None).unwrap(), 2.5);
        // Population sd of 1,2,3,4 = sqrt(1.25); the sample sd would be sqrt(5/3).
        let sd = compute(&t, "ODEStdDev", "x", None).unwrap();
        assert!((sd - 1.25f64.sqrt()).abs() < 1e-15, "{sd}");
    }

    #[test]
    fn ode_value_interpolates_and_clamps() {
        let t = cooling_table();
        // Exactly on a sample.
        assert_eq!(
            compute(&t, "ODEValue", "temp", Some(20.0)).unwrap(),
            47.59095803046333
        );
        // Halfway between the first two rows.
        let mid = compute(&t, "odevalue", "temp", Some(10.0)).unwrap();
        assert!(
            (mid - (95.0 + 47.59095803046333) / 2.0).abs() < 1e-12,
            "{mid}"
        );
        // Outside the window: clamped to the nearest end, not extrapolated.
        assert_eq!(compute(&t, "ODEValue", "temp", Some(-5.0)).unwrap(), 95.0);
        assert_eq!(
            compute(&t, "ODEValue", "temp", Some(1e9)).unwrap(),
            23.734030127668667
        );
    }

    #[test]
    fn time_at_finds_the_first_crossing_and_reports_a_miss() {
        let t = cooling_table();
        // Temp falls through 50 between t = 0 and t = 20.
        let hit = compute(&t, "TimeAt", "temp", Some(50.0)).unwrap();
        assert!(hit > 0.0 && hit < 20.0, "{hit}");
        // Linear bracket between (0, 95) and (20, 47.59095803046333).
        let expected = 20.0 * (95.0 - 50.0) / (95.0 - 47.59095803046333);
        assert!((hit - expected).abs() < 1e-12, "{hit} vs {expected}");

        // A sample sitting exactly on the target returns that sample's time.
        assert_eq!(compute(&t, "TimeAt", "temp", Some(95.0)).unwrap(), 0.0);

        let err = compute(&t, "TimeAt", "temp", Some(1000.0))
            .unwrap_err()
            .to_string_message();
        assert!(err.contains("TimeAt: column never reaches 1000"), "{err}");
        assert!(err.contains("DYNAMIC 'cool'"), "{err}");
    }

    #[test]
    fn a_two_argument_accessor_without_its_argument_is_refused() {
        let t = cooling_table();
        let err = compute(&t, "ODEValue", "temp", None)
            .unwrap_err()
            .to_string_message();
        assert!(err.contains("ODEValue requires a second argument"), "{err}");
        let err = compute(&t, "TimeAt", "temp", None)
            .unwrap_err()
            .to_string_message();
        assert!(err.contains("TimeAt('col', value)"), "{err}");
    }

    #[test]
    fn an_unknown_column_lists_the_ones_that_exist() {
        let err = compute(&cooling_table(), "MaxValue", "nope", None)
            .unwrap_err()
            .to_string_message();
        assert!(
            err.contains("column 'nope' not found in DYNAMIC 'cool'"),
            "{err}"
        );
        assert!(err.contains("Available columns: [time, temp]"), "{err}");
    }

    #[test]
    fn an_empty_table_is_refused_before_any_statistic() {
        let err = compute(&table(&["time", "x"], Vec::new()), "MaxValue", "x", None)
            .unwrap_err()
            .to_string_message();
        assert!(err.contains("produced no rows"), "{err}");
    }

    #[test]
    fn an_unknown_accessor_name_is_named() {
        let err = compute(&cooling_table(), "MedianValue", "temp", None)
            .unwrap_err()
            .to_string_message();
        assert!(err.contains("Unknown ODE accessor 'medianvalue'"), "{err}");
    }

    #[test]
    fn a_single_row_table_still_answers() {
        let t = table(&["time", "x"], vec![vec![0.0, 7.0]]);
        assert_eq!(compute(&t, "FinalValue", "x", None).unwrap(), 7.0);
        assert_eq!(compute(&t, "MaxValue", "x", None).unwrap(), 7.0);
        assert_eq!(compute(&t, "ODEValue", "x", Some(5.0)).unwrap(), 7.0);
        // No pair of rows to bracket a crossing.
        assert!(compute(&t, "TimeAt", "x", Some(7.0)).is_err());
    }

    // -- ownership -----------------------------------------------------------

    fn block(name: &str, states: &[&str], aux: &[&str]) -> DynamicSystem {
        let mut body: Vec<Equation> = states
            .iter()
            .map(|s| {
                Equation::new(
                    Expr::call("der", vec![Expr::var(*s)]),
                    Expr::var("rate"),
                    format!("der({s}) = rate"),
                )
            })
            .collect();
        body.extend(
            aux.iter()
                .map(|a| Equation::new(Expr::var(*a), Expr::var("gain"), format!("{a} = gain"))),
        );
        DynamicSystem {
            name: name.into(),
            options: DynamicOptions::defaults("time", 0.0, 1.0),
            body_equations: body,
            for_blocks: Vec::new(),
            initials: states
                .iter()
                .map(|s| InitialCondition {
                    state: (*s).into(),
                    indices: Vec::new(),
                    value: Expr::num(0.0),
                })
                .collect(),
            events: Vec::new(),
            source_text: String::new(),
        }
    }

    #[test]
    fn a_column_is_owned_by_name_or_by_array_base_name() {
        let shape = analyze(&block("b", &["t"], &["qdot"]));
        assert!(owns_column(&shape, "t"));
        assert!(owns_column(&shape, "qdot"));
        assert!(owns_column(&shape, "time")); // the time variable is column 0
                                              // The static analysis never expands the discretization, so an element
                                              // matches through its base name.
        assert!(owns_column(&shape, "t[4]"));
        assert!(owns_column(&shape, "qdot[2]"));
        assert!(!owns_column(&shape, "other"));
        assert!(!owns_column(&shape, "other[1]"));
        // A leading bracket is not a base name.
        assert!(!owns_column(&shape, "[1]"));
    }

    #[test]
    fn input_vars_for_column_finds_the_owning_block() {
        let systems = vec![block("a", &["h"], &[]), block("b", &["temp"], &["qdot"])];
        assert_eq!(input_vars_for_column(&systems, "qdot"), ["gain", "rate"]);
        assert_eq!(input_vars_for_column(&systems, "temp"), ["gain", "rate"]);
        assert!(input_vars_for_column(&systems, "nope").is_empty());
    }

    // -- the live bridge -----------------------------------------------------

    #[test]
    fn no_context_resolves_to_zero_rather_than_failing() {
        let values = Scope::new();
        assert_eq!(
            resolve(None, "MaxValue", "anything", None, &values).unwrap(),
            0.0
        );
    }

    #[test]
    fn the_bridge_integrates_once_per_signature_and_caches() {
        // `rate` is the block's only input variable (`der(temp) = rate`), so it
        // is the only value in the cache signature.
        let systems = vec![block("b", &["temp"], &[])];
        assert_eq!(
            analyze(&systems[0])
                .input_vars
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            ["rate"]
        );
        let runs = std::cell::Cell::new(0usize);
        let ctx = DynamicAccessorContext::install(
            &systems,
            BTreeMap::new(),
            Box::new(|_ds: &DynamicSystem, values: &Scope| {
                runs.set(runs.get() + 1);
                let rate = values.get("rate").copied().unwrap_or(0.0);
                Ok(table(
                    &["time", "temp"],
                    vec![vec![0.0, 0.0], vec![1.0, rate]],
                ))
            }),
        );

        let mut values = Scope::new();
        values.insert("rate".into(), 10.0);
        assert_eq!(
            ctx.resolve("MaxValue", "temp", None, &values).unwrap(),
            10.0
        );
        assert_eq!(runs.get(), 1);
        // Same signature: served from the cache.
        assert_eq!(
            ctx.resolve("FinalValue", "temp", None, &values).unwrap(),
            10.0
        );
        assert_eq!(runs.get(), 1);
        // A changed input variable invalidates it.
        values.insert("rate".into(), 20.0);
        assert_eq!(
            ctx.resolve("MaxValue", "temp", None, &values).unwrap(),
            20.0
        );
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn a_value_outside_the_signature_does_not_invalidate_the_cache() {
        // Only the block's own input variables key the cache; an unrelated
        // analytic variable moving is not a reason to re-integrate.
        let systems = vec![block("b", &["temp"], &[])];
        let runs = std::cell::Cell::new(0usize);
        let ctx = DynamicAccessorContext::install(
            &systems,
            BTreeMap::new(),
            Box::new(|_ds: &DynamicSystem, _v: &Scope| {
                runs.set(runs.get() + 1);
                Ok(cooling_table())
            }),
        );
        let mut values = Scope::new();
        values.insert("rate".into(), 1.0);
        assert_eq!(
            ctx.resolve("MaxValue", "temp", None, &values).unwrap(),
            95.0
        );
        values.insert("unrelated".into(), 999.0);
        assert_eq!(
            ctx.resolve("MaxValue", "temp", None, &values).unwrap(),
            95.0
        );
        assert_eq!(runs.get(), 1);
    }

    #[test]
    fn a_dotted_display_name_resolves_to_the_flat_column() {
        let systems = vec![block("b", &["m$port$t"], &[])];
        let mut map = BTreeMap::new();
        map.insert("m.port.t".to_string(), "m$port$t".to_string());
        let ctx = DynamicAccessorContext::install(
            &systems,
            map,
            Box::new(|_ds: &DynamicSystem, _v: &Scope| {
                Ok(table(
                    &["time", "m$port$t"],
                    vec![vec![0.0, 1.0], vec![1.0, 5.0]],
                ))
            }),
        );
        let values = Scope::new();
        assert_eq!(
            ctx.resolve("MaxValue", "m.port.t", None, &values).unwrap(),
            5.0
        );
        // The flat name still works.
        assert_eq!(
            ctx.resolve("MaxValue", "m$port$t", None, &values).unwrap(),
            5.0
        );
    }

    #[test]
    fn a_column_no_block_owns_is_named() {
        let systems = vec![block("b", &["temp"], &[])];
        let ctx = DynamicAccessorContext::install(
            &systems,
            BTreeMap::new(),
            Box::new(|_ds: &DynamicSystem, _v: &Scope| Ok(cooling_table())),
        );
        let err = ctx
            .resolve("MaxValue", "nope", None, &Scope::new())
            .unwrap_err()
            .to_string_message();
        assert!(
            err.contains("no DYNAMIC block has a column 'nope'"),
            "{err}"
        );
    }

    #[test]
    fn a_nested_resolution_during_integration_yields_zero() {
        // The runner asks the context to resolve while it is integrating; the
        // guard must answer 0.0 rather than recursing forever.
        let systems = vec![block("b", &["temp"], &[])];
        let nested = std::cell::Cell::new(f64::NAN);
        let ctx = DynamicAccessorContext::install(
            &systems,
            BTreeMap::new(),
            Box::new(|_ds: &DynamicSystem, _v: &Scope| Ok(cooling_table())),
        );
        // Simulate the engine handing the context down into the inner solve.
        let outer = ctx
            .resolve("MaxValue", "temp", None, &Scope::new())
            .unwrap();
        assert_eq!(outer, 95.0);
        ctx.integrating.set(true);
        nested.set(
            ctx.resolve("MaxValue", "temp", None, &Scope::new())
                .unwrap(),
        );
        ctx.integrating.set(false);
        assert_eq!(nested.get(), 0.0);
    }

    #[test]
    fn a_runner_failure_propagates_and_is_not_cached() {
        let systems = vec![block("b", &["temp"], &[])];
        let calls = std::cell::Cell::new(0usize);
        let ctx = DynamicAccessorContext::install(
            &systems,
            BTreeMap::new(),
            Box::new(|_ds: &DynamicSystem, _v: &Scope| {
                calls.set(calls.get() + 1);
                Err(FreesError::solver("boom"))
            }),
        );
        let values = Scope::new();
        assert!(ctx.resolve("MaxValue", "temp", None, &values).is_err());
        assert!(ctx.resolve("MaxValue", "temp", None, &values).is_err());
        assert_eq!(calls.get(), 2);
        // The guard was restored despite the failure.
        assert!(!ctx.integrating.get());
    }

    #[test]
    fn input_vars_for_column_on_the_context_matches_the_free_function() {
        let systems = vec![block("b", &["temp"], &["qdot"])];
        let ctx = DynamicAccessorContext::install(
            &systems,
            BTreeMap::new(),
            Box::new(|_ds: &DynamicSystem, _v: &Scope| Ok(cooling_table())),
        );
        assert_eq!(
            ctx.input_vars_for_column("qdot"),
            input_vars_for_column(&systems, "qdot")
        );
        assert!(ctx.input_vars_for_column("nope").is_empty());
    }

    #[test]
    fn table_events_are_untouched_by_the_accessors() {
        // Guards the column indexing against an off-by-one if events grow.
        let mut t = cooling_table();
        t.events.push(TableEventHit {
            name: "cold".into(),
            time: 18.3,
        });
        assert_eq!(compute(&t, "MaxValue", "temp", None).unwrap(), 95.0);
    }
}
