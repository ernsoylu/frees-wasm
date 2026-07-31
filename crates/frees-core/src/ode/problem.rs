//! The ODE value types: the problem handed to the driver, the raw result it
//! returns, and the first-class **ODE Table** the `DYNAMIC` block publishes.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/core/ode/OdeProblem.java`,
//! `OdeResult.java`, `OdeRhs.java`, `OdeScalarFn.java` and `OdeTableResult.java`.
//!
//! # Two callbacks, both fallible
//!
//! The Java `OdeRhs` / `OdeScalarFn` are `@FunctionalInterface`s returning
//! `double[]` / `double` and *throwing* on failure. For a `DYNAMIC` block the
//! RHS closure is not arithmetic — it pins `t` and the states into the value
//! map, runs the algebraic inner solve, and reads the `der(...)` results back
//! out. That solve can fail. So the Rust callbacks return [`Result`] and every
//! caller propagates, which is the same control flow the Java exception gives.
//!
//! # `OdeEvent` lives here, not in [`crate::ode::events`]
//!
//! `OdeProblem` structurally contains `List<OdeEvent>`, so the type has to be
//! visible from this module for the problem to be constructible. `OdeEvent.java`
//! is 47 lines of pure data + two predicates; it is ported verbatim below.
//! [`crate::ode::events`] — which owns the *document-level* wiring (parsing
//! `EVENT name: lhs = rhs | direction -> action` into switching functions) —
//! should re-export [`OdeEvent`] rather than define a second one.
//!
//! # No clock
//!
//! The Java `OdeProblem` carries a `deadlineNanos` budget that
//! [`crate::ode::integrator`] checks on every step. `wasm32-unknown-unknown`
//! has no clock, exactly as [`crate::integral`] documents for `IntegralSolver`,
//! so the field is absent and the step budget
//! ([`crate::ode::integrator::MAX_STEPS`]) is the only bound.

use core::fmt;

use crate::diag::Result;

/// Output sample count when the header does not give one — the Java
/// `OdeProblem.sampleCount()` literal, which equals
/// `DynamicSystem.Options.DEFAULT_POINTS`.
pub const DEFAULT_SAMPLE_COUNT: usize = 200;

/// Ceiling on the number of output rows one `DYNAMIC` block may materialise.
///
/// **A divergence from the Java, added deliberately** (`docs/status-phase1.md`
/// ledger item 19). `OdeIntegrator.integrate` allocates `double[sampleCount]`
/// plus a `double[dimension]` per sample straight from the header's `points`,
/// with no ceiling: `points = 1e9` asks for an 8 GB `times` array before a
/// single step is taken. On the JVM that surfaces as an `OutOfMemoryError` the
/// web layer turns into a 500; in wasm32 the allocation simply fails and the
/// `panic = "abort"` profile **kills the worker**, which no `Result` can catch
/// and no reload short of a new page can recover.
///
/// The value is `MAX_RANGE_ELEMENTS`, the ceiling `parser::toplevel` already
/// applies to a materialised `PARAMETRIC` sweep — the same kind of object, and
/// itself a transcription of the Java `AstBuilder.MAX_RANGE_ELEMENTS`. The
/// largest `points` anywhere in the 390-document corpus is 1 201.
pub const MAX_OUTPUT_SAMPLES: usize = 100_000;

// ---------------------------------------------------------------------------
// Callbacks
// ---------------------------------------------------------------------------

/// The right-hand side of an explicit ODE system: given time `t` and the state
/// vector `y`, returns `dy/dt`.
///
/// Port of `OdeRhs.java`. For a frees `DYNAMIC` block this closure is built by
/// the orchestrator — it writes the state vector into the value map, solves the
/// algebraic auxiliary block with `t` and the states pinned (the shared
/// per-step inner solve), then evaluates each `der(...)` right-hand side. All
/// states therefore advance on one shared step cursor — the multi-state
/// capability the single-state `Integral()` lacks.
pub trait OdeRhs {
    fn eval(&self, t: f64, y: &[f64]) -> Result<Vec<f64>>;
}

impl<F> OdeRhs for F
where
    F: Fn(f64, &[f64]) -> Result<Vec<f64>>,
{
    fn eval(&self, t: f64, y: &[f64]) -> Result<Vec<f64>> {
        self(t, y)
    }
}

/// A scalar function of `(t, y)` over the ODE state, used for event switching
/// functions `g(t, y)` whose zero crossings are detected during integration.
///
/// Port of `OdeScalarFn.java`.
pub trait OdeScalarFn {
    fn eval(&self, t: f64, y: &[f64]) -> Result<f64>;
}

impl<F> OdeScalarFn for F
where
    F: Fn(f64, &[f64]) -> Result<f64>,
{
    fn eval(&self, t: f64, y: &[f64]) -> Result<f64> {
        self(t, y)
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// The discrete state reassignment a `set` event performs at its crossing.
///
/// The Java record fuses this into `OdeEvent` as `(setIndex, setValue)` with
/// `setIndex = -1` / `setValue = null` meaning "no reassignment"; pairing them
/// in one `Option` removes the `setIndex >= 0 && setValue == null` shape that
/// would dereference null in `OdeIntegrator.run`.
pub struct StateReset<'a> {
    /// Index into the state vector — `OdeEvent.setIndex()`.
    pub index: usize,
    /// The new value, evaluated at the crossing — `OdeEvent.setValue()`.
    pub value: Box<dyn OdeScalarFn + 'a>,
}

/// An event whose switching function `g(t, y)` is monitored for zero crossings
/// during integration.
///
/// Port of `OdeEvent.java`. `direction` selects which crossings fire: `+1`
/// rising (− to +), `−1` falling (+ to −), `0` any. When `stop` is true the
/// integration terminates at the crossing (e.g. apogee `v = 0`); otherwise the
/// crossing is only recorded.
pub struct OdeEvent<'a> {
    pub name: String,
    pub g: Box<dyn OdeScalarFn + 'a>,
    pub direction: i32,
    pub stop: bool,
    /// `Some` exactly when the Java `isSet()` is true.
    pub set: Option<StateReset<'a>>,
}

impl<'a> OdeEvent<'a> {
    /// stop/record event (no state reassignment) — the Java 4-arg constructor.
    pub fn new(
        name: impl Into<String>,
        g: Box<dyn OdeScalarFn + 'a>,
        direction: i32,
        stop: bool,
    ) -> OdeEvent<'a> {
        OdeEvent {
            name: name.into(),
            g,
            direction,
            stop,
            set: None,
        }
    }

    /// An event that reassigns state `index` to `value` at the crossing — the
    /// Java 6-arg canonical constructor with `setIndex >= 0`.
    pub fn with_set(
        name: impl Into<String>,
        g: Box<dyn OdeScalarFn + 'a>,
        direction: i32,
        stop: bool,
        index: usize,
        value: Box<dyn OdeScalarFn + 'a>,
    ) -> OdeEvent<'a> {
        OdeEvent {
            name: name.into(),
            g,
            direction,
            stop,
            set: Some(StateReset { index, value }),
        }
    }

    /// Whether this event reassigns a state at the crossing (a `set` action).
    pub fn is_set(&self) -> bool {
        self.set.is_some()
    }

    /// Whether a sign change from `g_prev` to `g_new` matches this event's
    /// direction. Transcribed from `OdeEvent.triggers`: the four-way `crossed`
    /// disjunction is redundant as written in the Java and stays as written.
    pub fn triggers(&self, g_prev: f64, g_new: f64) -> bool {
        if g_prev == 0.0 && g_new == 0.0 {
            return false;
        }
        let crossed = (g_prev <= 0.0 && g_new > 0.0)
            || (g_prev >= 0.0 && g_new < 0.0)
            || (g_prev < 0.0 && g_new >= 0.0)
            || (g_prev > 0.0 && g_new <= 0.0);
        if !crossed {
            return false;
        }
        match self.direction {
            1 => g_new > g_prev,  // rising through zero
            -1 => g_new < g_prev, // falling through zero
            _ => true,
        }
    }
}

/// `OdeEvent.directionFromKeyword` — `rising` → `+1`, `falling` → `−1`,
/// anything else (including absent) → `0`.
pub fn direction_from_keyword(keyword: Option<&str>) -> i32 {
    match keyword.unwrap_or("any").to_ascii_lowercase().as_str() {
        "rising" => 1,
        "falling" => -1,
        _ => 0,
    }
}

impl fmt::Debug for OdeEvent<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OdeEvent")
            .field("name", &self.name)
            .field("direction", &self.direction)
            .field("stop", &self.stop)
            .field("set_index", &self.set.as_ref().map(|s| s.index))
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// The problem
// ---------------------------------------------------------------------------

/// A fully specified initial-value problem handed to
/// [`crate::ode::integrator::integrate`]: the state RHS, the integration
/// window, the chosen method and tolerances, the output sample count, and any
/// events to monitor. Numeric fields are SI.
///
/// Port of `OdeProblem.java`, minus `deadlineNanos` (see the module docs).
///
/// `max_step` is *not* defaulted here: `DynamicSolver.solve` passes
/// `(tf - t0) / 100` when the header omits `maxstep`, so that one adaptive step
/// cannot grow large enough to straddle an event. Callers building an
/// `OdeProblem` for a `DYNAMIC` block must apply that default themselves.
pub struct OdeProblem<'a> {
    /// Solver name (`ode1`–`ode5`, `ode45`, `ode23`, `ode23s`, `ode15s`, …).
    pub method: String,
    /// Start time.
    pub t0: f64,
    /// End time.
    pub tf: f64,
    /// Initial state vector.
    pub y0: Vec<f64>,
    /// `dy/dt` closure.
    pub rhs: &'a dyn OdeRhs,
    /// Output sample count (>= 2); `None` uses [`DEFAULT_SAMPLE_COUNT`].
    pub points: Option<usize>,
    /// Fixed step size; `None` means adaptive (where supported).
    pub fixed_step: Option<f64>,
    /// Relative tolerance (adaptive/stiff).
    pub rtol: f64,
    /// Absolute tolerance (adaptive/stiff).
    pub atol: f64,
    /// Cap on a single step; `None` means unbounded.
    pub max_step: Option<f64>,
    /// Zero-crossing events.
    pub events: Vec<OdeEvent<'a>>,
}

impl OdeProblem<'_> {
    /// `OdeProblem.sampleCount()` — the header's `points` when it is at least
    /// 2, otherwise [`DEFAULT_SAMPLE_COUNT`].
    pub fn sample_count(&self) -> usize {
        match self.points {
            Some(p) if p >= 2 => p,
            _ => DEFAULT_SAMPLE_COUNT,
        }
    }

    /// `OdeProblem.dimension()` — the number of differential states.
    pub fn dimension(&self) -> usize {
        self.y0.len()
    }
}

impl fmt::Debug for OdeProblem<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OdeProblem")
            .field("method", &self.method)
            .field("t0", &self.t0)
            .field("tf", &self.tf)
            .field("y0", &self.y0)
            .field("points", &self.points)
            .field("fixed_step", &self.fixed_step)
            .field("rtol", &self.rtol)
            .field("atol", &self.atol)
            .field("max_step", &self.max_step)
            .field("events", &self.events)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// The raw result
// ---------------------------------------------------------------------------

/// A single event firing: its name, the crossing time, and the state there.
///
/// Port of `OdeResult.EventRecord`.
#[derive(Debug, Clone, PartialEq)]
pub struct EventRecord {
    pub name: String,
    pub time: f64,
    pub state: Vec<f64>,
}

/// The outcome of an ODE solve: the state trajectory sampled at evenly spaced
/// output times (using the driver's dense interpolant), plus any recorded
/// events. `end_time` is `tf` unless a `stop` event fired earlier, in which
/// case `stopped` is true and the trajectory ends at the crossing.
///
/// Port of `OdeResult.java`.
#[derive(Debug, Clone, PartialEq)]
pub struct OdeResult {
    /// Sampled output times, length = number of samples.
    pub times: Vec<f64>,
    /// `states[i]` is the state vector at `times[i]`.
    pub states: Vec<Vec<f64>>,
    /// Recorded event hits in time order.
    pub events: Vec<EventRecord>,
    /// Whether a stop-event terminated the integration early.
    pub stopped: bool,
    /// The final time reached.
    pub end_time: f64,
    /// Accepted internal steps (diagnostics).
    pub accepted_steps: usize,
    /// Rejected internal steps (diagnostics).
    pub rejected_steps: usize,
}

impl OdeResult {
    /// `OdeResult.dimension()`.
    pub fn dimension(&self) -> usize {
        self.states.first().map_or(0, Vec::len)
    }
}

// ---------------------------------------------------------------------------
// The ODE Table
// ---------------------------------------------------------------------------

/// One recorded event firing in an [`OdeTableResult`] — name and time only.
///
/// Port of `OdeTableResult.EventHit`. Distinct from
/// [`EventRecord`] (which also carries the state) and from the driver's
/// internal crossing record.
#[derive(Debug, Clone, PartialEq)]
pub struct TableEventHit {
    pub name: String,
    pub time: f64,
}

/// The first-class *ODE Table* produced by one solved `DYNAMIC` block — a
/// sibling of the Parametric Table / Function Table family.
///
/// Port of `OdeTableResult.java`. Columns are `[timeVar, states…,
/// auxiliaries…]` and rows are the sampled time steps, shaped so the frontend
/// renders it in the Tables window and plots it (state vs time / state vs
/// state) through the existing parametric-table path with no new plot code. The
/// analytic solver also reads cells/extrema out of it via the ODE Table
/// accessors (`ODEValue`, `FinalValue`, `MaxValue`, `TimeAt`, column
/// aggregates) in a second-solve pass.
///
/// **A solved `DYNAMIC` block puts nothing in the solution's `variables` map.**
/// This table is the whole trajectory, and it is what the golden fixtures'
/// `ode_tables` array records; a transient fixture that compares only
/// `variables` passes vacuously.
#[derive(Debug, Clone, PartialEq)]
pub struct OdeTableResult {
    /// Block name (the table / graph name).
    pub name: String,
    /// Column headers, `[timeVar, states…, auxiliaries…]`.
    pub columns: Vec<String>,
    /// One row per output sample, aligned to `columns`.
    pub rows: Vec<Vec<f64>>,
    /// Recorded event firings (name + time).
    pub events: Vec<TableEventHit>,
    /// The solver actually used.
    pub method: String,
    /// Whether a stop-event ended the run early.
    pub stopped: bool,
    /// Final time reached.
    pub end_time: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn const_fn(v: f64) -> Box<dyn OdeScalarFn + 'static> {
        Box::new(move |_t: f64, _y: &[f64]| Ok(v))
    }

    #[test]
    fn sample_count_floors_at_two_and_defaults_to_200() {
        let rhs = |_t: f64, _y: &[f64]| Ok(vec![0.0]);
        let mut p = OdeProblem {
            method: "ode45".into(),
            t0: 0.0,
            tf: 1.0,
            y0: vec![0.0],
            rhs: &rhs,
            points: None,
            fixed_step: None,
            rtol: 1e-6,
            atol: 1e-9,
            max_step: None,
            events: Vec::new(),
        };
        assert_eq!(p.sample_count(), DEFAULT_SAMPLE_COUNT);
        p.points = Some(0);
        assert_eq!(p.sample_count(), DEFAULT_SAMPLE_COUNT);
        p.points = Some(1);
        assert_eq!(p.sample_count(), DEFAULT_SAMPLE_COUNT);
        p.points = Some(2);
        assert_eq!(p.sample_count(), 2);
        p.points = Some(41);
        assert_eq!(p.sample_count(), 41);
        assert_eq!(p.dimension(), 1);
    }

    #[test]
    fn direction_keywords_match_the_java_switch() {
        assert_eq!(direction_from_keyword(Some("rising")), 1);
        assert_eq!(direction_from_keyword(Some("RISING")), 1);
        assert_eq!(direction_from_keyword(Some("falling")), -1);
        assert_eq!(direction_from_keyword(Some("Falling")), -1);
        assert_eq!(direction_from_keyword(Some("any")), 0);
        assert_eq!(direction_from_keyword(Some("nonsense")), 0);
        assert_eq!(direction_from_keyword(None), 0);
    }

    #[test]
    fn triggers_honours_direction_and_ignores_flat_zero() {
        let any = OdeEvent::new("any", const_fn(0.0), 0, false);
        let rising = OdeEvent::new("up", const_fn(0.0), 1, false);
        let falling = OdeEvent::new("down", const_fn(0.0), -1, false);

        // Flat on zero is not a crossing.
        assert!(!any.triggers(0.0, 0.0));

        // Rising through zero.
        assert!(any.triggers(-1.0, 1.0));
        assert!(rising.triggers(-1.0, 1.0));
        assert!(!falling.triggers(-1.0, 1.0));

        // Falling through zero.
        assert!(any.triggers(1.0, -1.0));
        assert!(!rising.triggers(1.0, -1.0));
        assert!(falling.triggers(1.0, -1.0));

        // Touching zero from below counts as a rising crossing.
        assert!(rising.triggers(-1.0, 0.0));
        assert!(!falling.triggers(-1.0, 0.0));

        // No sign change at all.
        assert!(!any.triggers(1.0, 2.0));
        assert!(!any.triggers(-2.0, -1.0));
    }

    #[test]
    fn is_set_matches_the_constructor_used() {
        let plain = OdeEvent::new("e", const_fn(1.0), 0, true);
        assert!(!plain.is_set());
        let reset = OdeEvent::with_set("e", const_fn(1.0), 0, false, 2, const_fn(9.0));
        assert!(reset.is_set());
        assert_eq!(reset.set.as_ref().unwrap().index, 2);
        assert_eq!(reset.set.unwrap().value.eval(0.0, &[]).unwrap(), 9.0);
    }

    #[test]
    fn result_dimension_reads_the_first_row() {
        let empty = OdeResult {
            times: Vec::new(),
            states: Vec::new(),
            events: Vec::new(),
            stopped: false,
            end_time: 0.0,
            accepted_steps: 0,
            rejected_steps: 0,
        };
        assert_eq!(empty.dimension(), 0);
        let two = OdeResult {
            times: vec![0.0],
            states: vec![vec![1.0, 2.0]],
            ..empty
        };
        assert_eq!(two.dimension(), 2);
    }

    /// The `[timeVar, states…, auxiliaries…]` column contract, pinned to a real
    /// oracle dump so the layer that *builds* the table has something to check
    /// against. From `tools/golden-dumper/run.sh` on:
    ///
    /// ```text
    /// kc   = 0.05
    /// Tinf = 20
    /// mcp  = 3.0
    /// DYNAMIC cooling (method = ode45, time = 0 .. 60, points = 4)
    ///   der(Temp) = -kc * (Temp - Tinf)
    ///   Temp(0)   = 95
    ///   qdot      = mcp * der(Temp)
    ///   excess    = Temp - Tinf
    ///   EVENT warm: Temp = 30 | falling -> record
    /// END
    /// ```
    ///
    /// Three things the golden establishes and this test records:
    /// * column order is time, then **states in declaration order**, then
    ///   **auxiliaries in declaration order** — `qdot` before `excess`;
    /// * every identifier is lowercased (`Temp` → `temp`), because frees names
    ///   are case-insensitive;
    /// * the golden's `variables` map held only `{Tinf, kc, mcp}` — the states
    ///   and auxiliaries appear *nowhere* outside this table.
    #[test]
    fn the_ode_table_matches_the_oracle_shape() {
        let table = OdeTableResult {
            name: "cooling".into(),
            columns: vec!["time".into(), "temp".into(), "qdot".into(), "excess".into()],
            rows: vec![
                vec![0.0, 95.0, -11.25, 75.0],
                vec![
                    20.0,
                    47.590_958_030_463_33,
                    -4.138_643_704_569_5,
                    27.590_958_030_463_327,
                ],
                vec![
                    40.0,
                    30.150_146_238_537_44,
                    -1.522_521_935_780_616_3,
                    10.150_146_238_537_442,
                ],
                vec![
                    60.0,
                    23.734_030_127_668_667,
                    -0.560_104_519_150_300_1,
                    3.734_030_127_668_667_4,
                ],
            ],
            events: vec![TableEventHit {
                name: "warm".into(),
                time: 40.298_060_374_733_27,
            }],
            method: "ode45".into(),
            stopped: false,
            end_time: 60.0,
        };

        assert_eq!(table.columns[0], "time", "column 0 is the time variable");
        assert_eq!(&table.columns[1..2], ["temp"], "states follow time");
        assert_eq!(
            &table.columns[2..],
            ["qdot", "excess"],
            "auxiliaries follow the states, in declaration order"
        );
        for (i, row) in table.rows.iter().enumerate() {
            assert_eq!(row.len(), table.columns.len(), "row {i} width");
        }
        // `points = 4` ⇒ four evenly spaced samples across the full span.
        assert_eq!(table.rows.len(), 4);
        assert_eq!(table.rows[0][0], 0.0);
        assert_eq!(table.rows[3][0], table.end_time);
        assert!(!table.stopped, "a `record` event does not stop the run");
        // The auxiliary is a function of the state at that row: qdot = mcp·der(Temp)
        // = 3·(−0.05·(temp − 20)).
        for row in &table.rows {
            let want = 3.0 * (-0.05 * (row[1] - 20.0));
            assert!((row[2] - want).abs() < 1e-12, "qdot from temp");
            assert!((row[3] - (row[1] - 20.0)).abs() < 1e-12, "excess from temp");
        }
    }

    #[test]
    fn closures_satisfy_both_callback_traits() {
        let rhs = |t: f64, y: &[f64]| Ok(vec![t + y[0]]);
        let r: &dyn OdeRhs = &rhs;
        assert_eq!(r.eval(2.0, &[3.0]).unwrap(), vec![5.0]);

        let g = |t: f64, y: &[f64]| Ok(t * y[0]);
        let s: &dyn OdeScalarFn = &g;
        assert_eq!(s.eval(2.0, &[3.0]).unwrap(), 6.0);
    }
}
