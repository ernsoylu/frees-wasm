//! End-to-end steady solve: source text in, solved variables out.
//!
//! This is the Rust counterpart of the two entry points the parent engine
//! exposes over HTTP (`../frEES/backend/web/.../CheckController.java` and
//! `SolveController`), both of which delegate to
//! `../frEES/backend/core/.../EquationSystemSolver.java`:
//!
//! * [`check`] mirrors `POST /api/check` — verify syntax and *structural*
//!   solvability (zero degrees of freedom plus a complete equation↔variable
//!   matching) and report the counts, **without solving anything**. The
//!   frontend gates its Solve button on this.
//! * [`solve`] mirrors `POST /api/solve` — the full pipeline
//!   `parse → collect equations → apply GUESS → block → Newton per block`.
//!
//! # The pipeline
//!
//! The stage order is the Java order, established by reading
//! `EquationParser.parseResult` and `EquationSystemSolver.solve`:
//! `parseResult` lexes/parses, collects `FUNCTION`/`PROCEDURE`/`MODULE`/`TABLE`
//! definitions, **expands components**, then **flattens statements** —
//! `CALL` statements become binding equations and matrix constructs become
//! scalar equations *at parse time* — and `solve` then applies
//! `ComplexExpansion.expand` (when complex mode is on) before blocking. Hence:
//!
//! 1. **Parse.** [`crate::parser::parse_document`]. A block construct the wasm
//!    port has not reached yet is an explicit error, never a silent skip
//!    (`SYMBOLIC` is refused here too).
//! 2. **Expand components.** [`expand_component_layer`] — the acausal
//!    `COMPONENT`/`connect` layer, at `EquationParser.parseResult:292-306`.
//!    It runs **before** every other expansion pass and seeds the equation
//!    list, because the Java writes `new BoundedEquationList(componentEquations)`
//!    and then flattens the statements *into* it. The component layer is a
//!    parser/expander, not a second solver: what comes out is flat scalar
//!    equations that take the same Newton/Tarjan path as everything else.
//! 3. **Flatten CALLs.** [`crate::procedures::flatten_calls`] — the CALL half
//!    of `EquationParser.flatten`. (Phase-4 stub: refuses CALLs by name.)
//! 4. **Expand matrices.** [`crate::parser::expand::expand_document`] — the
//!    matrix half of `EquationParser` (multiAssign, rangeAssign, linear-algebra
//!    intrinsics). Scalar documents pass through byte-identical.
//! 5. **Expand complex.** [`crate::parser::complex::expand_complex`] with
//!    [`SolverSettings::complex_mode`] — `EquationSystemSolver.solve`'s
//!    `ComplexExpansion.expand` site, *after* parse-time flattening.
//! 6. **Seed.** Every unknown starts at [`DEFAULT_GUESS`] (`1.0`, the Java
//!    `EquationSystemSolver.DEFAULT_GUESS`) with bounds `±∞`; in-text `GUESS`
//!    directives override the guess and narrow the bounds, and the guess is
//!    clamped into the bounds exactly as `withTextGuesses` does.
//! 7. **Block.** [`crate::solver::blocker::block_system`] — degrees of freedom,
//!    maximum bipartite matching, Tarjan SCC. The blocks come out in solve
//!    order.
//! 8. **Solve.** [`crate::solver::newton::newton_solve`] per block, in that
//!    order, inside the Java retry ladder (see `solve_block_with_fallback`
//!    below): transformed-guess retries, a univariate bracketing rescue, block
//!    merging, then a best-effort polish pass. Residuals evaluate through
//!    [`crate::eval::eval_with`] with the document's definitions in context,
//!    so user `FUNCTION`s and `TABLE`s work the moment their evaluator lands.
//!    Values solved by an earlier block are already in the shared scope
//!    when a later block is evaluated, which is what "feed knowns forward"
//!    means here — a downstream block never re-solves an upstream unknown.
//!
//! # Built-in constants fold at parse time
//!
//! Like the Java `AstBuilder.visitVarAtom`, the expression parser substitutes
//! `pi#`/`R#`/`g#` (every name [`crate::eval::lookup_constant`] knows) as
//! numeric literals carrying their raw SI unit string, so a constant never
//! reaches the solver as a variable and the unit checker can ground downstream
//! variables from it. [`builtin_constants`] survives as a knowns hook — it
//! collects nothing on documents from this parser (folding leaves no `#`
//! variables behind) but keeps hand-built ASTs and the component expander
//! honest.
//!
//! # Evaluation failures inside the Newton loop
//!
//! [`crate::eval`] turns domain errors (division by zero, `ln` of a
//! non-positive, `sqrt` of a negative) into hard errors where Java returns
//! `NaN`. That is the right call for a user-facing expression evaluation and
//! the wrong one inside a line search, where the solver *expects* to probe
//! invalid regions and back off. The engine bridges the two: the residuals are
//! evaluated once at the initial point and any failure there is reported
//! verbatim (that is where a genuinely broken document — unknown function,
//! wrong arity, a string used as a number — shows up); after that, an
//! `Evaluation`/`Property` failure is reported to Newton as a non-finite
//! residual, which is the signal its Jacobian probing and step halving are
//! already written to handle.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::ast::{Equation, Expr, Statement};
use crate::diag::{Diagnostic, FreesError, Result};
use crate::differentiator::differentiate;
use crate::eval::{eval_with, lookup_constant, EvalContext, Scope};
use crate::integral::IntegralEquation;
use crate::parser::defs::Definitions;
use crate::parser::{parse_document, Document, GuessDirective};
use crate::procedures::flatten_calls_into;
use crate::solver::blocker::{block_system, unknowns, Block};
use crate::solver::newton::{newton_solve_problem, NewtonProblem, SolverSettings};
use crate::units::registry::UnitRegistry;

/// Initial value for an unknown with no `GUESS`.
///
/// `EquationSystemSolver.DEFAULT_GUESS` in the Java engine.
pub const DEFAULT_GUESS: f64 = 1.0;

/// One equation's residual `lhs - rhs` at the final solution.
///
/// The Java `EquationSystemSolver.EquationResidual` (`equation`, `residual`),
/// plus the index of the Tarjan block the equation was solved in, which the
/// Java side reconstructs from `Block.equations` when it needs it
/// (`failedBlockIndex` diagnostics in `api.ts`).
#[derive(Debug, Clone, PartialEq)]
pub struct EquationResidual {
    /// The equation quoted verbatim (`Equation.source_text` — the Java
    /// `Equation::sourceText`), never a mangled internal form.
    pub equation: String,
    /// `lhs - rhs` evaluated at the returned values. `NaN` when the equation
    /// cannot be evaluated there (Java catches and records `NaN` the same way
    /// in `enrichWithPartialResult`).
    pub residual: f64,
    /// 0-based index into [`Solution::blocks`] of the block that solved it.
    pub block: usize,
}

/// Solve-effort statistics — the subset of the Java `EquationSystemSolver.Stats`
/// the core can know. `equationCount`/`unknownCount`/`blockCount` are derivable
/// from the [`Solution`] itself (`residuals.len()`, `values.len()`,
/// `blocks.len()`), so they are not duplicated here.
#[derive(Debug, Clone, PartialEq)]
pub struct SolveStats {
    /// Total Newton iterations across every block (Java `Stats.iterations`).
    pub iterations: usize,
    /// Largest `|residual|` across the system at the returned solution
    /// (Java `Stats.maxResidual`).
    pub max_residual: f64,
    /// Wall-clock solve time (Java `Stats.elapsedMillis`). Always `None` in
    /// core: `wasm32-unknown-unknown` has no clock, so the boundary stage
    /// measures and fills this where a clock exists.
    pub elapsed_ms: Option<f64>,
}

/// A completed steady solve.
#[derive(Debug, Clone, PartialEq)]
pub struct Solution {
    /// Every **unknown** in the system with its solved value, keyed by the
    /// lowercase canonical name (frees identifiers are case-insensitive).
    ///
    /// Built-in `#` constants are deliberately *not* listed: the Java engine
    /// substitutes them as numeric literals at parse time, so they are never
    /// result rows (`fixtures/golden/constants.json` lists only `a`, `b`, `c`).
    pub values: BTreeMap<String, f64>,
    /// Lowercase canonical name → the spelling of its **first appearance** in
    /// the source, sigil suffixes kept as-is (the Java
    /// `ParseResult.displayNames`): `t_out` → `"T_out"`. Covers exactly the
    /// unknowns in [`Solution::values`].
    pub display_names: BTreeMap<String, String>,
    /// The blocks, in the order they were solved.
    pub blocks: Vec<Block>,
    /// Per block (aligned with [`Solution::blocks`]), the `source_text` of its
    /// equations, first occurrence of a repeated text kept — the Java
    /// `SolveDtos.toBlockDto` applies `.distinct()` so an expanded component
    /// never lists one user line twice.
    pub block_equations: Vec<Vec<String>>,
    /// `lhs - rhs` of every equation, in source order, evaluated at the final
    /// values (the Java `Result.residuals`).
    pub residuals: Vec<EquationResidual>,
    /// Effort statistics for the whole solve.
    pub stats: SolveStats,
    /// Lowercase variable name → display unit string: declared units
    /// (annotated literals, then external `VariableInfo` units on top) plus the
    /// units [`crate::units::checker`] derived dimensionally — the Java
    /// `SolverApiSupport.unitsByLowerName` composition, which fills the
    /// `variables[].units` column of the solve response.
    pub inferred_units: BTreeMap<String, String>,
    /// Unit-consistency warning sentences from [`crate::units::checker`], in
    /// Java emission order (`unitWarnings[]` in `api.ts`). Unit problems are
    /// warnings by the parent engine's invariant — they never block a solve.
    pub unit_warnings: Vec<String>,
    /// Parser diagnostics plus anything the solve wanted to say. Unit and
    /// bounds problems are warnings and never block a solve.
    pub diagnostics: Vec<Diagnostic>,
    /// Total Newton iterations across every block (same value as
    /// `stats.iterations`, kept for existing callers).
    pub iterations: usize,
    /// The document's **top-level** `COMPONENT` instantiations, in declaration
    /// order — the datasheet's input half.
    ///
    /// The Java `SolveController` re-parses `cleanText` and calls
    /// `ComponentMetadata.build(cleanText, variableDtos)`, which reads
    /// `program.componentInsts()`: the *unflattened*, top-level list. This
    /// carries the same list forward off the one parse instead, so the boundary
    /// does not re-lex the document.
    ///
    /// Empty for a document with no component layer.
    pub component_instances: Vec<crate::components::metadata::ComponentInstMeta>,
    /// The connection topology of the expanded network — the schematic's data
    /// layer (the Java `ParseResult.componentConnections`). Empty for a document
    /// with no component layer.
    pub component_connections: Vec<crate::components::expander::Connection>,
}

/// Diagnostics captured at the point a block solve gave up — the Java
/// `SolverException.partialResult` built by `enrichWithPartialResult`: the
/// block structure, every equation's residual evaluated at the stalled
/// iterate (`NaN` where an unsolved upstream block leaves nothing to
/// evaluate), and partial stats. Deliberately carries **no solved values** —
/// the Java partial `Result` ships `Map.of()` for `variables`.
#[derive(Debug, Clone, PartialEq)]
pub struct PartialDiagnostics {
    /// The full block decomposition (known before the first block ran).
    pub blocks: Vec<Block>,
    /// Per block, the `source_text` of its equations (see
    /// [`Solution::block_equations`]).
    pub block_equations: Vec<Vec<String>>,
    /// Lowercase canonical name → first-seen source spelling. This is the whole
    /// `ParseResult.displayNames` map the Java partial `Result` carries, so it
    /// may name more identifiers than the system has unknowns — use
    /// [`PartialDiagnostics::unknown_count`] for the count.
    pub display_names: BTreeMap<String, String>,
    /// How many unknowns the stalled system had (the Java partial
    /// `Stats.variableCount`, i.e. `surfacedVarCount`).
    pub unknown_count: usize,
    /// Every equation's `lhs - rhs` at the stalled iterate, source order.
    pub residuals: Vec<EquationResidual>,
    /// Iterations spent before the stall + the largest finite `|residual|`.
    pub stats: SolveStats,
}

/// A failed solve: what went wrong, plus the structured diagnostics the Java
/// engine carries on `SolverException` (`FailureState.failedBlockIndex` +
/// `partialResult`). Pre-block failures (parse errors, structural rejections)
/// have `failed_block_index: None` and `partial: None`, exactly as Java
/// failures without a `FailureState` pass through unenriched.
#[derive(Debug, Clone, PartialEq)]
pub struct SolveFailure {
    /// The underlying error (its message already carries the block annotation
    /// where one applies).
    pub error: FreesError,
    /// 0-based index of the Tarjan block whose solve gave up.
    pub failed_block_index: Option<usize>,
    /// Boxed to keep the `Err` variant small on every `solve` return path
    /// (`clippy::result_large_err` — the payload is ~180 bytes inline).
    pub partial: Option<Box<PartialDiagnostics>>,
}

impl SolveFailure {
    /// The message without the `Display` kind prefix (delegates to
    /// [`FreesError::to_string_message`]).
    pub fn to_string_message(&self) -> String {
        self.error.to_string_message()
    }

    /// Source span of the underlying error, where one is known.
    pub fn span(&self) -> Option<crate::diag::Span> {
        self.error.span()
    }
}

impl std::fmt::Display for SolveFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for SolveFailure {}

impl From<FreesError> for SolveFailure {
    fn from(error: FreesError) -> SolveFailure {
        SolveFailure {
            error,
            failed_block_index: None,
            partial: None,
        }
    }
}

/// Lets existing assertions compare a failure against a bare [`FreesError`]
/// (`assert_eq!(solve(..).unwrap_err(), FreesError::solver("…"))`).
impl PartialEq<FreesError> for SolveFailure {
    fn eq(&self, other: &FreesError) -> bool {
        self.error == *other
    }
}

/// One syntax error with its 1-based editor position, so the lint gutter can
/// mark the broken line. The Java `SolveDtos.SyntaxErrorDto`.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxErrorInfo {
    /// 1-based line of the error.
    pub line: usize,
    /// 1-based column of the error.
    pub column: usize,
    pub message: String,
}

/// The result of a check-before-solve.
///
/// Field-for-field the Java `EquationSystemSolver.CheckResult`
/// (`solvable`, `equationCount`, `unknownCount`, `variables`, `message`), plus
/// the parser's diagnostics, which the Java side carries on a separate field of
/// the HTTP DTO.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckReport {
    /// True when the system is structurally solvable: zero degrees of freedom
    /// and a complete equation↔variable matching.
    pub solvable: bool,
    pub equation_count: usize,
    pub unknown_count: usize,
    /// The unknowns, sorted, lowercase.
    pub variables: Vec<String>,
    /// Lowercase canonical name → first-seen source spelling, covering exactly
    /// [`CheckReport::variables`]. See [`Solution::display_names`].
    pub display_names: BTreeMap<String, String>,
    /// Human-readable summary — the "No syntax errors were detected…" sentence
    /// on success, the blocker's causality diagnosis on a structural failure,
    /// or `"Syntax error: …"` on a parse failure (the Java `CheckController`
    /// 400-with-body message).
    pub message: String,
    /// 1-based editor line of the syntax error this report describes, `None`
    /// for whole-system problems (the `errorLine` field `api.ts` parses).
    pub error_line: Option<usize>,
    /// Every syntax error with its position. Empty unless the parse failed.
    /// (The Rust parser stops at the first error, so at most one entry today;
    /// the Java engine collects up to eight.)
    pub errors: Vec<SyntaxErrorInfo>,
    /// Lowercase variable name → inferred display unit (`inferredUnits` in
    /// `api.ts`): units [`crate::units::checker`] derived dimensionally,
    /// overlaid by units read off annotated literal assignments — the Java
    /// `CheckController` composition (`deriveUnits` then `putAll(inferUnits)`),
    /// which deliberately leaves external `VariableInfo` units out.
    pub inferred_units: BTreeMap<String, String>,
    /// Unit-consistency warning sentences from [`crate::units::checker`], in
    /// Java emission order (`unitWarnings[]` in `api.ts`). Never affects
    /// [`CheckReport::solvable`].
    pub unit_warnings: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Externally supplied per-variable solver information — one row of the
/// Variable Information window, the Java `SolverApiSupport.VariableInfoDto`
/// (`name`, `guess`, `lower`, `upper`, `units`; `uncertainty` is not ported).
///
/// Values are expressed in `unit` when one is given and are converted to SI
/// (`value * factor + offset`) before the solver sees them, exactly as
/// `VariableInfoDto.toSpec` does.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VariableOverride {
    /// Variable name, any case (lowercased on use).
    pub name: String,
    pub guess: Option<f64>,
    pub lower: Option<f64>,
    pub upper: Option<f64>,
    /// Display unit the numbers above are written in. `None`, `""` and `"-"`
    /// all mean "already SI"; an *unknown* unit falls back to factor 1 /
    /// offset 0 silently, matching the Java `toSpec` catch-and-default.
    pub unit: Option<String>,
}

/// What the document says about one unknown before the solve starts.
///
/// The Java `VariableSpec` carries more (display name, unit, format); only the
/// three fields the solver reads are needed here.
#[derive(Debug, Clone, Copy, PartialEq)]
struct VarSpec {
    guess: f64,
    lower: f64,
    upper: f64,
}

impl Default for VarSpec {
    fn default() -> VarSpec {
        VarSpec {
            guess: DEFAULT_GUESS,
            lower: f64::NEG_INFINITY,
            upper: f64::INFINITY,
        }
    }
}

impl VarSpec {
    /// The starting value: the guess clamped into the declared bounds, as
    /// `withTextGuesses` does with `Math.clamp`.
    fn initial(&self) -> f64 {
        self.guess.clamp(self.lower, self.upper)
    }

    fn is_bounded(&self) -> bool {
        self.lower.is_finite() || self.upper.is_finite()
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Parse, block and solve `source`. Equivalent to [`solve_with`] with no
/// external variable information.
///
/// # Errors
///
/// * [`FreesError::Parse`] — a syntax error, or a block construct the wasm port
///   has not implemented (`COMPONENT`, `PROCEDURE`, `PARAMETRIC`, …).
/// * [`FreesError::Solver`] — no equations, nonzero degrees of freedom, a
///   structurally singular system, or a block that would not converge.
/// * whatever [`crate::eval`] raises when the residuals cannot be evaluated at
///   the initial point (unknown function, wrong arity, a string in a numeric
///   position).
pub fn solve(
    source: &str,
    settings: &SolverSettings,
) -> std::result::Result<Solution, SolveFailure> {
    solve_with(source, settings, &[])
}

/// [`solve`] with externally supplied per-variable guesses/bounds — the shape
/// `POST /api/solve` receives as `variableInfo`.
///
/// # How overrides and in-text `GUESS` directives merge
///
/// The rule is the Java one, verified against
/// `EquationSystemSolver.withTextGuesses` and `VariableInfoDto.toSpec`:
///
/// 1. Each override converts to a spec first (values pass through its `unit`
///    to SI). An override with bounds but no guess starts from `DEFAULT_GUESS`
///    clamped into those bounds.
/// 2. In-text `GUESS` directives then merge **over** the overrides, field by
///    field: a part the directive states wins; a part it omits falls back to
///    the override ("text wins, so a shared document solves identically for
///    its recipient").
/// 3. The merged guess is clamped into the merged bounds — a stale external
///    guess landing outside text bounds is pulled onto them (the bounds win).
///
/// # Errors
///
/// Everything [`solve`] raises, plus [`FreesError::Solver`] for an invalid
/// override (NaN, crossed bounds, or an explicit guess outside its own
/// bounds), mirroring the Java `VariableSpec` constructor's rejections.
pub fn solve_with(
    source: &str,
    settings: &SolverSettings,
    overrides: &[VariableOverride],
) -> std::result::Result<Solution, SolveFailure> {
    crate::props::tables::install_builtin_once();
    let mut doc = parse_document(source)?;
    reject_unsupported(&doc)?;

    // Pipeline stage 1b — the acausal component layer, at the Java position:
    // `EquationParser.parseResult` expands components *before* `flatten`, and
    // seeds the equation list with the result. See `expand_component_layer`.
    let mut diagnostics = doc.diagnostics.clone();
    let mut components = expand_component_layer(&mut doc, &mut diagnostics)?;

    // Pipeline stages 2–4, in the Java order (see the module docs): CALL
    // flattening happens at parse time in `EquationParser.flatten`, matrix
    // expansion alongside it, and `ComplexExpansion.expand` runs in
    // `EquationSystemSolver.solve` after both.
    let statements = std::mem::take(&mut doc.statements);
    let mut parsed_names = std::mem::take(&mut doc.display_names);
    doc.statements = flatten_calls_into(statements, &doc.defs, &mut parsed_names)?;
    doc.display_names = parsed_names;
    // The Java: `List<Equation> equations = new BoundedEquationList(componentEquations)`,
    // then `flatten(statements, …, equations, …)` appends into it. Component
    // equations therefore come FIRST, and the residual list, block ordering and
    // `block_equations` all inherit that order.
    let mut equations = std::mem::take(&mut components.equations);
    equations.extend(crate::parser::expand::expand_document(&doc)?);
    let defs = &doc.defs;

    // Pipeline stage 3b — the Integral pass, at the Java position (`solve`
    // runs `IntegralSolver.hoistNested` then `findIntegrals` on the flattened
    // equations, ahead of `ComplexExpansion` and of blocking). A document that
    // mentions no `Integral` comes out of `hoist_nested` byte-identical and
    // takes the same path it always did.
    let equations = crate::integral::hoist_nested(equations);
    let integrals = find_integrals(&equations, defs, settings.complex_mode)?;
    let mut stepping_iterations = 0usize;
    let equations = if integrals.is_empty() {
        if settings.complex_mode {
            // `ComplexExpansion.expand(equations, parsed.displayNames())` — the
            // Java threads the map in so `x_r`/`x_i` display as
            // `<display of x>_r` / `_i` rather than falling back to themselves.
            let mut complex_names: HashMap<String, String> = doc
                .display_names
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let expanded =
                crate::parser::complex::expand_with_display_names(&equations, &mut complex_names)?;
            doc.display_names = complex_names.into_iter().collect();
            expanded
        } else {
            crate::parser::complex::expand_complex(equations, false)?
        }
    } else {
        // The stepping driver needs guesses and bounds *before* the final
        // equation list exists (it solves pinned subsystems to build it), so
        // the specs are materialised twice: here over the hoisted system — a
        // superset, and where the Java reads them from the parse result ahead
        // of everything — and again below over the lowered system, which is
        // what the result rows are keyed on. This pass's diagnostics are
        // dropped so a `GUESS` warning is never emitted twice.
        let (_, hoisted_knowns) = builtin_constants(&equations);
        let mut dropped = Vec::new();
        let pre_specs = variable_specs(&equations, &hoisted_knowns, &doc, overrides, &mut dropped)?;
        let (lowered, driven) =
            lower_integrals(&equations, &integrals, settings, &pre_specs, defs)?;
        stepping_iterations = driven;
        lowered
    };

    let (constants, knowns) = builtin_constants(&equations);
    let report = block_system(&equations, &knowns)?;

    // `diagnostics` already carries the parser's own plus anything the component
    // layer said (it was seeded before expansion so a steady-storage rewrite is
    // reported even when a later stage fails).
    collect_unit_warnings(&equations, &mut diagnostics);
    let specs = variable_specs(&equations, &knowns, &doc, overrides, &mut diagnostics)?;

    // One scope for the whole document: it starts as the initial guesses plus
    // the built-in constants, and each block overwrites its own unknowns as it
    // is solved. That *is* the "feed solved values forward" mechanism — a later
    // block reading `p` sees the value the earlier block determined.
    let mut values: Scope = HashMap::with_capacity(specs.len() + constants.len());
    values.extend(constants.iter().map(|(k, v)| (k.clone(), *v)));
    for (name, spec) in &specs {
        values.insert(name.clone(), spec.initial());
    }

    let iterations = match run_blocks(
        &report.blocks,
        &equations,
        &mut values,
        settings,
        &specs,
        defs,
    ) {
        Ok(block_iterations) => stepping_iterations + block_iterations,
        Err(BlockLoopFailure {
            error,
            failed_block_index,
            iterations,
        }) => {
            // The Java `enrichWithPartialResult`: attach the block structure,
            // every equation's residual at the stalled iterate (`residuals_at`
            // records NaN where evaluation fails), and partial stats, so a
            // failure ships diagnostics.
            let (residuals, max_residual) = residuals_at(&equations, &report.blocks, &values, defs);
            let block_equations = block_equation_texts(&report.blocks, &equations);
            let display_names = complete_display_names(&doc.display_names, &equations);
            return Err(SolveFailure {
                error,
                failed_block_index: Some(failed_block_index),
                partial: Some(Box::new(PartialDiagnostics {
                    blocks: report.blocks,
                    block_equations,
                    display_names,
                    unknown_count: surfaced_count(specs.keys().map(String::as_str)),
                    residuals,
                    stats: SolveStats {
                        iterations: stepping_iterations + iterations,
                        max_residual,
                        elapsed_ms: None,
                    },
                })),
            });
        }
    };

    check_bounds(&specs, &values, &mut diagnostics);

    // Report the *unknowns*, not the whole scope. The built-in constants were
    // seeded into the scope so the evaluator could read them (see the module
    // docs), but the Java engine substitutes them as literals at parse time and
    // never surfaces them as result variables — `fixtures/golden/constants.json`
    // lists only `a`, `b`, `c`. Leaking `pi#` into the result table would be a
    // visible parity divergence.
    //
    // Ignored-output sinks are dropped for the same reason: an omitted trailing
    // CALL output (`CALL LinFit(x, y : m, b)`) is backed by a real unknown that
    // the solver must still determine, but Java never surfaces it — the
    // `EquationSystemSolver` result loop does `if (isIgnoredSink(name)) return;`.
    let solved: BTreeMap<String, f64> = specs
        .keys()
        .filter(|name| !crate::parser::toplevel::is_ignored_sink(name))
        .map(|name| {
            let value = values.get(name).copied().unwrap_or(f64::NAN);
            (name.clone(), value)
        })
        .collect();

    let display_names = complete_display_names(&doc.display_names, &equations);
    let (residuals, max_residual) = residuals_at(&equations, &report.blocks, &values, defs);
    let block_equations = block_equation_texts(&report.blocks, &equations);

    // Dimensional check + SI unit inference (the Java solve path's
    // `checkUnits` + `unitsByLowerName`): declared units feed the checker, and
    // the result map is the derived units overlaid by the declared ones —
    // a declared unit always wins over a dimensionally derived one.
    let declared = declared_units(&equations, overrides, &components.member_units);
    let unit_report = crate::units::checker::check_units(&equations, &declared);
    let mut inferred_units = unit_report.inferred;
    inferred_units.extend(declared);

    Ok(Solution {
        values: solved,
        display_names,
        blocks: report.blocks,
        block_equations,
        residuals,
        stats: SolveStats {
            iterations,
            max_residual,
            // No clock on wasm32-unknown-unknown; the boundary stage measures.
            elapsed_ms: None,
        },
        inferred_units,
        unit_warnings: unit_report.warnings,
        diagnostics,
        iterations,
        component_instances: components.instances,
        component_connections: components.connections,
    })
}

/// Verify syntax and structural solvability without solving. Equivalent to
/// [`check_with`] with no external variable information.
pub fn check(source: &str) -> Result<CheckReport> {
    check_with(source, &[])
}

/// Verify syntax and structural solvability without solving.
///
/// Mirrors `POST /api/check`. *Every* failure of the document itself is data,
/// not an `Err`:
///
/// * a **syntax** failure (or an unported construct) returns a report with
///   `solvable: false`, `error_line`/`errors` pointing at the offending line
///   and a `"Syntax error: …"` message — the body the Java `CheckController`
///   sends with its 400, which `api.ts` parses like any other check result;
/// * a **structural** failure (degrees of freedom, singular matching) returns
///   `solvable: false` with the blocker's diagnosis in `message`, because the
///   editor needs the counts and the variable list either way.
///
/// `overrides` cannot change a check's outcome — the Java check endpoint never
/// receives `variableInfo`, and structural solvability does not depend on
/// guesses. They are still *validated*, so a caller preparing a solve gets the
/// same early `Err` surface from both entry points.
pub fn check_with(source: &str, overrides: &[VariableOverride]) -> Result<CheckReport> {
    crate::props::tables::install_builtin_once();
    for o in overrides {
        override_spec(o)?;
    }

    let doc = match parse_document(source).and_then(|doc| {
        reject_unsupported(&doc)?;
        Ok(doc)
    }) {
        Ok(doc) => doc,
        Err(err @ FreesError::Parse { .. }) => return Ok(syntax_failure_report(source, &err)),
        Err(other) => return Err(other),
    };

    // The same expansion pipeline as `solve_with` (stages 2–4; Java `check`
    // runs `ComplexExpansion` too — complex mode is off here because the
    // check entry point carries no settings, exactly like the Java
    // `check(source)` overload defaulting `complexMode` to false). A failing
    // pass answers as report data, mirroring the two Java surfaces: a
    // `ParseException` becomes the 400-with-body syntax report, and anything
    // else the `catch (SolverException)` not-solvable report with the counts
    // of the equations as they stood when the pass refused.
    // Display names accumulate across the pipeline exactly as on the solve
    // path; the closure below works on a clone of the document, so the names
    // the CALL flattener generates are collected out here.
    let mut parsed_names = doc.display_names.clone();
    let mut check_diagnostics = doc.diagnostics.clone();
    let mut member_units: BTreeMap<String, String> = BTreeMap::new();
    let expanded = (|| {
        let mut doc = doc.clone();
        // Stage 1b — the component layer, at the Java position (see
        // `expand_component_layer`). `check` runs it for the same reason
        // `solve` does: without it a component document has zero equations and
        // would be reported "solvable" with nothing in it.
        let components = expand_component_layer(&mut doc, &mut check_diagnostics)?;
        member_units = components.member_units;
        parsed_names = doc.display_names.clone();
        let statements = std::mem::take(&mut doc.statements);
        doc.statements = flatten_calls_into(statements, &doc.defs, &mut parsed_names)?;
        let mut equations = components.equations;
        equations.extend(crate::parser::expand::expand_document(&doc)?);
        // The Integral pass, as in `solve_with` — but `check` builds the
        // *structural view* instead of driving the quadrature: a constant-limit
        // integral contributes a `resultVar = 0` placeholder, a variable-limit
        // one its inlined equation, and each integration variable is pinned
        // once. Without that pin `F = Integral(t^2, t, 0, 1)` is one equation
        // in two unknowns and the blocker would reject a valid document.
        let equations = crate::integral::hoist_nested(equations);
        let integrals = find_integrals(&equations, &doc.defs, false)?;
        if integrals.is_empty() {
            crate::parser::complex::expand_complex(equations, false)
        } else {
            crate::integral::structural_view(&equations, &integrals)
        }
    })();
    let equations: Vec<Equation> = match expanded {
        Ok(equations) => equations,
        Err(err @ FreesError::Parse { .. }) => return Ok(syntax_failure_report(source, &err)),
        Err(err) => {
            let equations: Vec<Equation> = doc.equations().into_iter().cloned().collect();
            let (_, knowns) = builtin_constants(&equations);
            let variables = unknowns(&equations, &knowns);
            return Ok(CheckReport {
                solvable: false,
                equation_count: equations.len(),
                unknown_count: variables.len(),
                display_names: complete_display_names(&parsed_names, &equations),
                variables,
                message: err.to_string_message(),
                error_line: None,
                errors: Vec::new(),
                inferred_units: BTreeMap::new(),
                unit_warnings: Vec::new(),
                diagnostics: check_diagnostics,
            });
        }
    };
    let (_, knowns) = builtin_constants(&equations);
    let all_vars = unknowns(&equations, &knowns);
    // Java `check`: report the *surfaced* balance. Each ignored-output sink adds
    // one variable and the one equation that determines it, so hiding the sink
    // without also hiding its equation would report a bogus `n equations and
    // n-1 variables`. Java subtracts both
    // (`surfacedEqs = equations.size() - (allVars.size() - surfacedVars)`).
    let surfaced_vars = surfaced_count(all_vars.iter().map(String::as_str));
    let surfaced_eqs = equations.len() - (all_vars.len() - surfaced_vars);
    let variables: Vec<String> = all_vars
        .into_iter()
        .filter(|name| !crate::parser::toplevel::is_ignored_sink(name))
        .collect();

    let mut diagnostics = check_diagnostics;
    collect_unit_warnings(&equations, &mut diagnostics);

    // Dimensional check + SI unit inference (the Java `CheckController` path):
    // declared units — annotated literals plus external `VariableInfo` units —
    // feed the checker; the reported map is the derived units overlaid by the
    // *literal*-declared ones only. External units are deliberately left out of
    // the report (the caller already knows them), exactly as the Java check
    // response composes `deriveUnits` + `inferUnits`.
    //
    // Component stream members are the exception the Java makes explicitly:
    // `CheckController.addComponentMemberUnits` puts them back into the
    // *reported* map too (`putIfAbsent`), because a port member's unit comes
    // from its physical domain and nothing else can derive it.
    let declared = declared_units(&equations, overrides, &member_units);
    let unit_report = crate::units::checker::check_units(&equations, &declared);
    let mut inferred_units = unit_report.inferred;
    inferred_units.extend(literal_units(&equations));
    for (name, unit) in &member_units {
        inferred_units
            .entry(name.to_ascii_lowercase())
            .or_insert_with(|| unit.clone());
    }

    let base = CheckReport {
        solvable: false,
        equation_count: surfaced_eqs,
        unknown_count: surfaced_vars,
        display_names: complete_display_names(&parsed_names, &equations),
        variables,
        message: String::new(),
        error_line: None,
        errors: Vec::new(),
        inferred_units,
        unit_warnings: unit_report.warnings,
        diagnostics,
    };

    match block_system(&equations, &knowns) {
        Ok(_) => Ok(CheckReport {
            solvable: true,
            message: format!(
                "No syntax errors were detected. There are {} equations and {} variables.",
                base.equation_count, base.unknown_count
            ),
            ..base
        }),
        Err(err) => Ok(CheckReport {
            message: err.to_string_message(),
            ..base
        }),
    }
}

// ---------------------------------------------------------------------------
// Pipeline steps
// ---------------------------------------------------------------------------

/// Refuse anything that parses but that the engine cannot honour.
fn reject_unsupported(doc: &Document) -> Result<()> {
    reject_unsupported_statements(&doc.statements)
}

/// Everything the acausal component layer contributes to a solve.
///
/// Built by [`expand_component_layer`] and consumed in three places: the
/// equations seed the equation list, `member_units` grounds the unit checker,
/// and the last two fields ride out on [`Solution`] for the datasheet and the
/// schematic.
#[derive(Debug, Default)]
struct ComponentLayer {
    /// The expanded scalar equations — component bodies then `connect` nodes.
    equations: Vec<Equation>,
    /// Flat solver name → SI unit (`s2$p` → `Pa`), the Java
    /// `ParseResult.componentMemberUnits`.
    member_units: BTreeMap<String, String>,
    /// The top-level instantiations, for [`Solution::component_instances`].
    instances: Vec<crate::components::metadata::ComponentInstMeta>,
    /// The connection topology, for [`Solution::component_connections`].
    connections: Vec<crate::components::expander::Connection>,
}

/// Expand the acausal `COMPONENT`/`connect` layer into flat scalar equations
/// and rewrite the document's own dotted references onto the same flat names.
///
/// # Where this sits in the pipeline, and why
///
/// **Established by reading `EquationParser.parseResult`** (`../frEES/backend/
/// core/src/main/java/com/frees/backend/parser/EquationParser.java:265-345`),
/// which runs, in order:
///
/// ```text
///   AstBuilder.buildProgram(program)                       // parse
///   new ComponentExpander(ComponentLibrary.builtins(), …)  // <- HERE
///   componentEquations = components.expand()
///   statements         = components.rewriteStatements(statements)
///   …storage routing (steadyStorageEquations / routeStorageIntoDynamic)…
///   List<Equation> equations = new BoundedEquationList(componentEquations);
///   flatten(statements, …, equations, …)                   // CALL + matrix + FOR
///   equations = StringVariables.resolve(equations, displayNames)
/// ```
///
/// and only then does `EquationSystemSolver.solve` run `ComplexExpansion.expand`
/// and block. So expansion is **the first expansion pass**: before CALL
/// flattening, before matrix expansion, before complex, and well before
/// blocking. Two consequences are load-bearing rather than incidental:
///
/// * the component equations are the **seed** of the list (`new
///   BoundedEquationList(componentEquations)`), so they come *before* every
///   equation the document itself wrote — this port prepends for the same
///   reason, and the residual list, block ordering and `block_equations` all
///   inherit that order;
/// * `rewriteStatements` runs on the statements *before* they are flattened, so
///   a dotted `P1.out.h` inside a `FOR` body or a `CALL` argument is rewritten
///   once, at the AST level, and the later passes only ever see flat names.
///
/// # Storage
///
/// `hasStorage()` (any component body with `der(member) = …`) routes into the
/// document's `DYNAMIC` block in the Java. `DYNAMIC` is not parsed by this port
/// yet (`parser/toplevel.rs` lists it under `unsupported_construct`), so
/// `dynamicSystems` is always empty here and the Java's *other* branch applies
/// verbatim: the §8.2 steady/transient duality, where each `der(X) = rhs`
/// becomes the equilibrium constraint `rhs = 0` and the state is an ordinary
/// unknown. That is [`steady_storage_equations`].
fn expand_component_layer(
    doc: &mut Document,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<ComponentLayer> {
    // The Java constructs a `ComponentExpander` unconditionally, but its
    // `ComponentLibrary.builtins()` is a static already paid for at class-load.
    // Here the 122 KB of embedded library text is parsed lazily on first use, so
    // a document with no component layer must not touch it at all — otherwise
    // every scalar solve in the corpus pays for 295 component definitions.
    if doc.components.is_empty() {
        return Ok(ComponentLayer::default());
    }

    let components = std::mem::take(&mut doc.components);
    let statements = std::mem::take(&mut doc.statements);
    let mut display_names = std::mem::take(&mut doc.display_names);

    let builtins = crate::components::library::builtins()?;
    let (equations, statements, member_units, connections) = {
        let mut expander = crate::components::expander::ComponentExpander::new(
            builtins.defs(),
            &components.defs,
            &components.instances,
            &components.connects,
            &mut display_names,
        )?;
        let equations = expander.expand()?;
        let statements = expander.rewrite_statements(statements)?;
        let member_units = expander
            .member_units()
            .into_iter()
            .map(|(name, unit)| (name, unit.to_string()))
            .collect();
        // `hasStorage()` with no DYNAMIC block: the Java's steady branch.
        let equations = if expander.has_storage() {
            steady_storage_equations(equations, diagnostics)
        } else {
            equations
        };
        (equations, statements, member_units, expander.connections())
    };

    doc.statements = statements;
    doc.display_names = display_names;
    let instances = components
        .instances
        .iter()
        .map(crate::components::metadata::ComponentInstMeta::from)
        .collect();

    Ok(ComponentLayer {
        equations,
        member_units,
        instances,
        connections,
    })
}

/// `der(X) = rhs` → `rhs = 0`, the Java `EquationParser.steadyStorageEquations`.
///
/// A storage network with no `DYNAMIC` block solves its **steady operating
/// point**: the state stops changing, so its derivative equation becomes the
/// equilibrium constraint and `X` is determined by the rest of the network.
/// The Java appends `" [steady: der=0]"` to the equation's source text; that is
/// kept verbatim so the block listing says which equations were rewritten.
///
/// The Java is silent about it. This port adds one *warning* diagnostic (never
/// an error — the parent invariant is that a document like this solves), because
/// in the browser there is no server log to read afterwards and the answer is
/// materially different from a transient run.
fn steady_storage_equations(
    equations: Vec<Equation>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Equation> {
    let mut rewritten = 0usize;
    let out: Vec<Equation> = equations
        .into_iter()
        .map(|eq| {
            let is_der = matches!(
                &eq.lhs,
                Expr::Call { function, args } if function == "der" && args.len() == 1
            );
            if !is_der {
                return eq;
            }
            rewritten += 1;
            Equation::new(
                eq.rhs,
                Expr::num(0.0),
                format!("{} [steady: der=0]", eq.source_text),
            )
        })
        .collect();
    if rewritten > 0 {
        diagnostics.push(Diagnostic::warning(format!(
            "This component network stores energy or mass ({rewritten} der(...) \
             equation(s)) but the document declares no DYNAMIC block, so it was \
             solved for its steady operating point (each der(X) = rhs became \
             rhs = 0)."
        )));
    }
    out
}

/// Refuse a statement that parses but that the engine cannot honour.
///
/// `Document::equations` walks past `SYMBOLIC` without comment. That is fine
/// for a structural walk and fatal for a solve: dropping it would silently
/// change what the document means. `CALL` statements are no longer refused
/// here — they belong to [`crate::procedures::flatten_calls`] (pipeline stage
/// 2), whose Phase-4 stub still refuses them by name, so nothing is ever
/// silently dropped.
fn reject_unsupported_statements(statements: &[Statement]) -> Result<()> {
    for statement in statements {
        match statement {
            Statement::Eq(_) | Statement::CallProc { .. } => {}
            Statement::For { body, .. } => reject_unsupported_statements(body)?,
            Statement::Symbolic(names) => {
                return Err(FreesError::parse(format!(
                    "`SYMBOLIC` is not supported by the wasm engine yet (declared: {})",
                    names.join(", ")
                )))
            }
        }
    }
    Ok(())
}

/// Every `#`-suffixed built-in the document mentions, as `(name, value)` pairs
/// plus the same names as a `knowns` set for the blocker.
///
/// See the module docs: without this a constant is an unmatched free variable.
fn builtin_constants(equations: &[Equation]) -> (BTreeMap<String, f64>, HashSet<String>) {
    let mut constants = BTreeMap::new();
    for equation in equations {
        for name in equation.variables() {
            if constants.contains_key(&name) {
                continue;
            }
            if let Some(constant) = lookup_constant(&name) {
                constants.insert(name, constant.value);
            }
        }
    }
    let knowns = constants.keys().cloned().collect();
    (constants, knowns)
}

/// Warn about every literal whose unit annotation the registry could not parse.
///
/// `parser::expr::number_literal` deliberately keeps the value and the user's
/// text when a unit is unknown — "a bad unit never fails a parse", and unit
/// problems are warnings by the parent engine's invariant. But it has nowhere
/// to *put* a warning, so as written `P = 140 [zorp]` silently solved to 140
/// with no indication that the annotation was ignored. Since a converted
/// literal always carries the SI display name and only an unconverted one
/// carries the original text, an annotation that still fails to parse is
/// exactly the set of literals that were left alone.
///
/// This is a stand-in for the unported `UnitChecker`, which additionally
/// verifies dimensional consistency across an equation.
fn collect_unit_warnings(equations: &[Equation], diagnostics: &mut Vec<Diagnostic>) {
    let mut seen = BTreeSet::new();
    for equation in equations {
        for side in [&equation.lhs, &equation.rhs] {
            walk_units(side, &mut |unit| {
                if UnitRegistry::parse(unit).is_err() && seen.insert(unit.to_string()) {
                    diagnostics.push(
                        Diagnostic::warning(format!(
                            "unknown unit `{unit}`: the literal was left unconverted, \
                             so this value is not in SI"
                        ))
                        .with_source_text(equation.source_text.clone()),
                    );
                }
            });
        }
    }
}

fn walk_units(expr: &Expr, report: &mut impl FnMut(&str)) {
    match expr {
        Expr::Num { unit: Some(u), .. } => report(u),
        Expr::Num { .. } | Expr::Str(_) | Expr::Var(_) => {}
        Expr::Neg(inner) | Expr::Not(inner) => walk_units(inner, report),
        Expr::BinOp { left, right, .. }
        | Expr::Compare { left, right, .. }
        | Expr::Logical { left, right, .. }
        | Expr::Range {
            start: left,
            end: right,
        } => {
            walk_units(left, report);
            walk_units(right, report);
        }
        Expr::ArrayLiteral(items) => items.iter().for_each(|e| walk_units(e, report)),
        Expr::Call { args, .. } => args.iter().for_each(|e| walk_units(e, report)),
        Expr::ArrayAccess { indices, .. } => indices.iter().for_each(|e| walk_units(e, report)),
    }
}

/// Split `a[1,2]` into `("a", "[1,2]")`; `None` for a name that is not an
/// expanded array/matrix element.
fn element_parts(name: &str) -> Option<(&str, &str)> {
    if !name.ends_with(']') {
        return None;
    }
    let open = name.find('[')?;
    (open > 0).then(|| name.split_at(open))
}

/// How many of `names` the user ever sees — the Java
/// `EquationSystemSolver.surfacedVarCount`. Ignored-output sinks are genuine
/// unknowns to the solver and invisible everywhere else.
fn surfaced_count<'a>(names: impl Iterator<Item = &'a str>) -> usize {
    names
        .filter(|name| !crate::parser::toplevel::is_ignored_sink(name))
        .count()
}

/// The Java `ParseResult.displayNames`, completed with what expansion adds.
///
/// The parser has already recorded every identifier the Java `AstBuilder`
/// registers (see [`crate::parser::Document::display_names`]) and the CALL
/// flattener has added its namespaced module variables. What remains is the
/// **element** rule, which the Java applies at four sites that all compute the
/// same thing (`EquationParser.expandExpr`'s `ArrayAccess` case,
/// `buildElementVars`, and the range expansions inside `parseMatrixInfo` /
/// `parseVectorInfo`, plus `flattenBareMatrixCreation`):
///
/// ```text
/// displayNames.put(base + "[i,j]",
///                  displayNames.getOrDefault(base, base) + "[i,j]")
/// ```
///
/// Because every one of those sites fires exactly when an element name enters
/// the equation list, replaying the rule over the expanded system's variables
/// reproduces the map without threading a mutable map through the whole
/// flattener. Note the Java uses `put`, not `putIfAbsent`: an element entry
/// always wins over anything with the same key, which is why this runs after
/// the parse-time map is in place and overwrites.
fn complete_display_names(
    parsed: &BTreeMap<String, String>,
    equations: &[Equation],
) -> BTreeMap<String, String> {
    let mut names = parsed.clone();
    let mut elements: BTreeSet<String> = BTreeSet::new();
    for equation in equations {
        elements.extend(
            equation
                .variables()
                .into_iter()
                .filter(|v| element_parts(v).is_some()),
        );
    }
    for element in elements {
        let Some((base, suffix)) = element_parts(&element) else {
            continue;
        };
        let base_display = names.get(base).cloned().unwrap_or_else(|| base.to_string());
        names.insert(element.clone(), format!("{base_display}{suffix}"));
    }
    names
}

/// The check report for a document that did not parse — the body the Java
/// `CheckController` returns with its 400 (`solvable=false`, zero counts,
/// `"Syntax error: …"`, `errorLine`, `errors`), which `api.ts` consumes like
/// any other check result.
fn syntax_failure_report(source: &str, err: &FreesError) -> CheckReport {
    let message = err.to_string_message();
    // The Java message is "Syntax error: " + the first line of the parser's
    // report (multi-line detail stays in `errors`/diagnostics).
    let first_line = message.lines().next().unwrap_or_default().to_string();

    let (error_line, errors) = match err.span() {
        Some(span) => {
            let (line, column) = span.line_col(source);
            (
                Some(line),
                vec![SyntaxErrorInfo {
                    line,
                    column,
                    message: message.clone(),
                }],
            )
        }
        // A parse refusal with no position (mirrors Java semantic
        // ParseExceptions, whose syntaxErrors() list is empty).
        None => (None, Vec::new()),
    };

    let mut diagnostic = Diagnostic::error(message);
    if let Some(span) = err.span() {
        diagnostic = diagnostic.with_span(span);
    }

    CheckReport {
        solvable: false,
        equation_count: 0,
        unknown_count: 0,
        variables: Vec::new(),
        display_names: BTreeMap::new(),
        message: format!("Syntax error: {first_line}"),
        error_line,
        errors,
        inferred_units: BTreeMap::new(),
        unit_warnings: Vec::new(),
        diagnostics: vec![diagnostic],
    }
}

/// Lowercase variable name → declared unit read off annotated literal
/// assignments: `P = 100 [bar]` declares `p`'s unit (as its SI display name,
/// since literals convert at parse time), first assignment wins. The Java
/// `EquationSystemSolver.inferUnits`.
fn literal_units(equations: &[Equation]) -> BTreeMap<String, String> {
    let mut units = BTreeMap::new();
    for equation in equations {
        let declared = match (&equation.lhs, &equation.rhs) {
            (
                Expr::Var(name),
                Expr::Num {
                    unit: Some(unit), ..
                },
            )
            | (
                Expr::Num {
                    unit: Some(unit), ..
                },
                Expr::Var(name),
            ) => Some((name, unit)),
            _ => None,
        };
        if let Some((name, unit)) = declared {
            units.entry(name.clone()).or_insert_with(|| unit.clone());
        }
    }
    units
}

/// Every unit declaration the unit checker should treat as ground truth: the
/// literal-annotated ones, overlaid by external `VariableInfo` units — an
/// explicit user unit always wins. The Java `SolverApiSupport.effectiveUnits`
/// (minus component member units, which wait on the component expander).
fn declared_units(
    equations: &[Equation],
    overrides: &[VariableOverride],
    component_member_units: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut units = literal_units(equations);
    // `SolverApiSupport.effectiveUnits`: inferred literal units, then the
    // component stream members' domain-derived units (`s2$p` → `Pa`), then the
    // explicit Variable-Information units, which win over both. A port member is
    // one of the solver's own unknowns, so nothing in the document derives its
    // unit — its physical domain is the only thing that fixes it, and without
    // this the checker would propagate *from* a dimensionless port and warn.
    units.extend(
        component_member_units
            .iter()
            .map(|(name, unit)| (name.to_ascii_lowercase(), unit.clone())),
    );
    for o in overrides {
        if let Some(unit) = o.unit.as_deref() {
            if !unit.trim().is_empty() {
                units.insert(o.name.trim().to_ascii_lowercase(), unit.to_string());
            }
        }
    }
    units
}

/// Every equation's residual at the final values, in source order, plus the
/// largest finite `|residual|` — the Java `buildResult` loop.
///
/// An equation that cannot be evaluated at the solution records `NaN` rather
/// than failing the whole solve (Java catches and stores `NaN` the same way);
/// non-finite residuals do not contribute to `max_residual`.
fn residuals_at(
    equations: &[Equation],
    blocks: &[Block],
    values: &Scope,
    defs: &Definitions,
) -> (Vec<EquationResidual>, f64) {
    // Which Tarjan block solved each equation. The blocker assigns every
    // equation of a square system to exactly one block; `unwrap_or(0)` is a
    // defensive default, not an expected path.
    let mut block_of: HashMap<usize, usize> = HashMap::with_capacity(equations.len());
    for (block_index, block) in blocks.iter().enumerate() {
        for &equation_index in &block.equations {
            block_of.insert(equation_index, block_index);
        }
    }

    let ctx = EvalContext { defs: Some(defs) };
    let mut residuals = Vec::with_capacity(equations.len());
    let mut max_residual = 0.0f64;
    for (index, equation) in equations.iter().enumerate() {
        let residual = match (
            eval_with(&equation.lhs, values, ctx),
            eval_with(&equation.rhs, values, ctx),
        ) {
            (Ok(lhs), Ok(rhs)) => lhs - rhs,
            _ => f64::NAN,
        };
        if residual.is_finite() {
            max_residual = max_residual.max(residual.abs());
        }
        residuals.push(EquationResidual {
            equation: equation.source_text.clone(),
            residual,
            block: block_of.get(&index).copied().unwrap_or(0),
        });
    }
    (residuals, max_residual)
}

/// Per block, the source text of its equations — first occurrence of a
/// repeated text kept, as the Java `toBlockDto`'s `.distinct()` does, so a
/// future component expansion never lists one user line twice.
fn block_equation_texts(blocks: &[Block], equations: &[Equation]) -> Vec<Vec<String>> {
    blocks
        .iter()
        .map(|block| {
            let mut seen = BTreeSet::new();
            block
                .equations
                .iter()
                .filter_map(|&index| equations.get(index))
                .filter(|equation| seen.insert(equation.source_text.as_str()))
                .map(|equation| equation.source_text.clone())
                .collect()
        })
        .collect()
}

/// Initial guesses and bounds for every unknown.
///
/// Port of `EquationSystemSolver.withTextGuesses` over the externally supplied
/// specs. **The merge rule, verified against the Java source:** the overrides
/// are applied first (they are the caller's `variableInfo` specs); each in-text
/// `GUESS` directive then merges *over* them field by field — a part the
/// directive states wins, a part it omits falls back to the override — and the
/// merged guess is clamped into the merged bounds. In-text wins on conflict;
/// the external value only survives where the directive is silent.
///
/// A directive naming something that is not an unknown is a warning, not an
/// error — the Java engine merges it into a spec map that the solver then never
/// reads. An *override* naming something that is not an unknown is silently
/// ignored, also the Java behaviour: the Variable Information window keeps
/// stale rows around and posts them with every request.
fn variable_specs(
    equations: &[Equation],
    knowns: &HashSet<String>,
    doc: &Document,
    overrides: &[VariableOverride],
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<BTreeMap<String, VarSpec>> {
    let mut specs: BTreeMap<String, VarSpec> = unknowns(equations, knowns)
        .into_iter()
        .map(|name| (name, VarSpec::default()))
        .collect();

    for o in overrides {
        let (name, spec) = override_spec(o)?;
        // Only real unknowns take a spec: `specs.keys()` defines the result
        // rows, so a stale override must not add a phantom variable.
        if let Some(slot) = specs.get_mut(&name) {
            *slot = spec;
        }
    }

    for guess in &doc.guesses {
        let name = guess.name.to_ascii_lowercase();
        match specs.get_mut(&name) {
            Some(spec) => apply_guess(spec, guess),
            None if knowns.contains(&name) => diagnostics.push(Diagnostic::warning(format!(
                "GUESS for `{name}`: that name is a built-in constant, \
                 not an unknown — the directive is ignored"
            ))),
            None => diagnostics.push(Diagnostic::warning(format!(
                "GUESS for `{name}`: no such variable in the system — \
                 the directive is ignored"
            ))),
        }
    }

    Ok(specs)
}

/// One override converted to `(lowercase name, spec)`, the port of
/// `VariableInfoDto.toSpec` plus the `VariableSpec` constructor's validation.
///
/// The numbers are written in `unit` and convert to SI as
/// `value * factor + offset`; an unknown unit falls back to factor 1 / offset 0
/// **silently**, exactly as the Java `toSpec` catch-and-default does — being
/// noisier here would be a visible behavioural divergence. A missing guess
/// defaults to [`DEFAULT_GUESS`] clamped into the bounds.
///
/// # Errors
///
/// The Java `VariableSpec` compact constructor's three rejections, as
/// [`FreesError::Solver`]: any NaN, a lower bound above the upper, or an
/// explicit guess outside its own bounds.
fn override_spec(o: &VariableOverride) -> Result<(String, VarSpec)> {
    let name = o.name.trim().to_ascii_lowercase();

    let (factor, offset) = match o.unit.as_deref().map(str::trim) {
        Some(unit) if !unit.is_empty() && unit != "-" => {
            match UnitRegistry::parse_with_offset(unit) {
                Ok(quantity) => (quantity.factor, quantity.offset),
                Err(_) => (1.0, 0.0),
            }
        }
        _ => (1.0, 0.0),
    };
    let to_si = |value: f64| value * factor + offset;

    let lower = o.lower.map(to_si).unwrap_or(f64::NEG_INFINITY);
    let upper = o.upper.map(to_si).unwrap_or(f64::INFINITY);
    let explicit_guess = o.guess.map(to_si);

    if lower.is_nan() || upper.is_nan() || explicit_guess.is_some_and(f64::is_nan) {
        return Err(FreesError::solver(format!(
            "Variable information for {name} contains NaN."
        )));
    }
    if lower > upper {
        return Err(FreesError::solver(format!(
            "Lower bound exceeds upper bound for variable {name}."
        )));
    }
    let guess = match explicit_guess {
        Some(guess) if guess < lower || guess > upper => {
            return Err(FreesError::solver(format!(
                "The guess value {guess} for variable {name} is outside \
                 its bounds [{lower}, {upper}]."
            )));
        }
        Some(guess) => guess,
        None => DEFAULT_GUESS.clamp(lower, upper),
    };

    Ok((
        name,
        VarSpec {
            guess,
            lower,
            upper,
        },
    ))
}

fn apply_guess(spec: &mut VarSpec, guess: &GuessDirective) {
    if let Some(lower) = guess.lower {
        spec.lower = lower;
    }
    if let Some(upper) = guess.upper {
        spec.upper = upper;
    }
    if let Some(value) = guess.guess {
        spec.guess = value;
    }
    // A reversed pair would make `clamp` panic; treat it as no ordering
    // information rather than aborting the solve over a typo in a hint.
    if spec.lower > spec.upper {
        std::mem::swap(&mut spec.lower, &mut spec.upper);
    }
}

// ---------------------------------------------------------------------------
// The per-block solve, its analytic Jacobian, and the Java retry ladder
// ---------------------------------------------------------------------------

/// The residual system of one block, evaluated against the shared scope, with
/// the per-entry symbolic derivatives where the differentiator produced them —
/// the object handed to [`newton_solve`].
struct BlockProblem<'a> {
    names: &'a [String],
    equations: &'a [&'a Equation],
    scope: &'a mut Scope,
    defs: &'a Definitions,
    /// `derivs[i][j] = ∂(lhs_i − rhs_i)/∂var_j` as an expression; `None`
    /// entries are structural zeros (equation `i` does not mention `var_j`).
    /// The whole field is `None` when any *dependent* entry failed to
    /// differentiate — the Java `analyticalJacobian` returning null, which
    /// pins the block to finite differences.
    derivs: Option<Vec<Vec<Option<Expr>>>>,
}

impl NewtonProblem for BlockProblem<'_> {
    fn residual(&mut self, x: &[f64], out: &mut [f64]) -> Result<()> {
        for (name, value) in self.names.iter().zip(x) {
            self.scope.insert(name.clone(), *value);
        }
        match residuals_into(self.equations, self.scope, self.defs, out) {
            Ok(()) => Ok(()),
            // An invalid region, not a broken document: hand Newton the
            // non-finite residual its line search knows how to reject.
            Err(FreesError::Evaluation { .. }) | Err(FreesError::Property { .. }) => {
                out.fill(f64::NAN);
                Ok(())
            }
            Err(other) => Err(other),
        }
    }

    /// Evaluate the pre-differentiated entries at `x` — the evaluation half of
    /// the Java `NewtonSolver.analyticalJacobian`. Any evaluation failure
    /// answers `None`, falling back to finite differences for this iteration
    /// exactly like the Java `catch` → `return null`.
    fn analytic_jacobian(&mut self, x: &[f64]) -> Option<Vec<Vec<f64>>> {
        let derivs = self.derivs.as_ref()?;
        for (name, value) in self.names.iter().zip(x) {
            self.scope.insert(name.clone(), *value);
        }
        let ctx = EvalContext {
            defs: Some(self.defs),
        };
        let n = self.names.len();
        let mut jacobian = vec![vec![0.0f64; n]; n];
        for (i, row) in derivs.iter().enumerate() {
            for (j, entry) in row.iter().enumerate() {
                if let Some(expr) = entry {
                    match eval_with(expr, self.scope, ctx) {
                        Ok(value) => jacobian[i][j] = value,
                        Err(_) => return None,
                    }
                }
            }
        }
        Some(jacobian)
    }
}

/// Pre-differentiate every dependent (equation, variable) pair of a block —
/// the symbolic half of the Java `NewtonSolver.analyticalJacobian`: residual
/// `lhs − rhs` differentiated w.r.t. each block variable, entries skipped for
/// equations that do not mention the variable (they stay structural zeros).
/// Returns `None` — no analytic source at all — as soon as any dependent
/// entry cannot be differentiated, the Java all-or-nothing fallback.
fn analytic_derivatives(
    block_equations: &[&Equation],
    variables: &[String],
) -> Option<Vec<Vec<Option<Expr>>>> {
    let n = variables.len();
    let mut derivs: Vec<Vec<Option<Expr>>> = vec![vec![None; n]; n];
    for (i, equation) in block_equations.iter().enumerate() {
        let mentioned = equation.variables();
        let residual_expr = equation.residual();
        for (j, var) in variables.iter().enumerate() {
            if !mentioned.contains(var) {
                continue; // structural zero — Java never differentiates these
            }
            {
                let d = differentiate(&residual_expr, var)?;
                derivs[i][j] = Some(d)
            }
        }
    }
    Some(derivs)
}

/// Solve one block in place, leaving its unknowns' values in `values`.
///
/// Returns the Newton iteration count. This is one rung's worth of work — the
/// Java `NewtonSolver.solveBlock`; the ladder around it lives in
/// [`solve_block_with_fallback`].
fn solve_block(
    index: usize,
    block: &Block,
    equations: &[Equation],
    values: &mut Scope,
    settings: &SolverSettings,
    specs: &BTreeMap<String, VarSpec>,
    defs: &Definitions,
) -> Result<usize> {
    let n = block.variables.len();
    if n == 0 {
        return Ok(0);
    }
    let block_equations: Vec<&Equation> = block
        .equations
        .iter()
        .filter_map(|&i| equations.get(i))
        .collect();
    if block_equations.len() != n {
        // The blocker guarantees this; a mismatch means the two modules
        // disagree, which must be loud rather than mysterious.
        return Err(FreesError::solver(format!(
            "internal error: block {} has {} equations for {} unknowns",
            index + 1,
            block_equations.len(),
            n
        )));
    }

    // Evaluate once at the initial point so a genuinely broken expression is
    // reported as itself instead of as "did not converge". See the module docs.
    let mut probe = vec![0.0; n];
    residuals_into(&block_equations, values, defs, &mut probe)
        .map_err(|err| annotate(err, index, &block_equations))?;

    let mut x: Vec<f64> = block
        .variables
        .iter()
        .map(|name| values.get(name).copied().unwrap_or(DEFAULT_GUESS))
        .collect();

    // Per-variable bounds from the specs — the Java IterationContext lo/hi
    // arrays, threaded into all three clamp sites inside `newton_solve`.
    let bounds: Vec<(f64, f64)> = block
        .variables
        .iter()
        .map(|name| {
            specs
                .get(name)
                .map(|spec| (spec.lower, spec.upper))
                .unwrap_or((f64::NEG_INFINITY, f64::INFINITY))
        })
        .collect();

    let names = &block.variables;
    let outcome = {
        let problem = BlockProblem {
            names,
            equations: &block_equations,
            scope: &mut *values,
            defs,
            derivs: analytic_derivatives(&block_equations, names),
        };
        newton_solve_problem(problem, &mut x, settings, Some(&bounds))
    };

    // Write back before propagating: on failure the last iterate is what makes
    // a stall report actionable, and it is what the Java engine leaves behind.
    for (name, value) in names.iter().zip(&x) {
        values.insert(name.clone(), *value);
    }

    let report = outcome.map_err(|err| annotate(err, index, &block_equations))?;
    Ok(report.iterations)
}

/// `out[k] = lhs_k(x) - rhs_k(x)` for each equation in the block, evaluated
/// with the document's definitions in context so user `FUNCTION`s and
/// `TABLE`s resolve (the Java `Evaluator.eval(expr, values, defs)` triple).
fn residuals_into(
    equations: &[&Equation],
    scope: &Scope,
    defs: &Definitions,
    out: &mut [f64],
) -> Result<()> {
    let ctx = EvalContext { defs: Some(defs) };
    for (slot, equation) in out.iter_mut().zip(equations) {
        *slot = eval_with(&equation.lhs, scope, ctx)? - eval_with(&equation.rhs, scope, ctx)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The retry ladder — port of EquationSystemSolver.solveBlockWithFallback
// ---------------------------------------------------------------------------

/// Retry attempts cap iterations: healthy Newton converges quickly, and a
/// generous user limit must not multiply across the whole ladder
/// (`EquationSystemSolver.MAX_RETRY_ITERATIONS`).
const MAX_RETRY_ITERATIONS: usize = 500;

/// Relative tolerance for accepting a bracketed root
/// (`EquationSystemSolver.BRACKET_RESIDUAL_TOL`): `|resid| / max(|lhs|, 1)`.
const BRACKET_RESIDUAL_TOL: f64 = 1.0e-6;

/// `EquationSystemSolver.retrySettings`: the ladder's retries run with the
/// user's stop criteria but the iteration budget capped at
/// [`MAX_RETRY_ITERATIONS`].
fn retry_settings(settings: &SolverSettings) -> SolverSettings {
    SolverSettings {
        max_iterations: settings.max_iterations.min(MAX_RETRY_ITERATIONS),
        ..*settings
    }
}

/// `EquationSystemSolver.polishSettings`: near-zero residual tolerance so the
/// polisher keeps iterating until the variable change drops below 1e-15
/// (`newton`'s `CHANGE_IN_VARIABLES`). This is critical for multiple roots,
/// where residual ≈ error^m drops below tolerance long before the variable
/// has converged. The Java record is `(50, 1e-30, 1e-15, elapsed, complex)`.
fn polish_settings(settings: &SolverSettings) -> SolverSettings {
    SolverSettings {
        max_iterations: 50,
        rel_tolerance: 1e-30,
        // Inert (strictly below every reachable `rel_tolerance * scale`), as
        // the Java criterion is purely relative.
        abs_tolerance: 0.0,
        ..*settings
    }
}

/// One alternative start for a failed block —
/// `EquationSystemSolver.GuessTransform`: a zero guess becomes ±`zero_offset`
/// (off the invariant manifold), a nonzero guess is rescaled (toward a distant
/// Newton basin), `conjugate` flips imaginary components (`_i`), and `jitter`
/// staggers each variable by its position to break exchange symmetries.
struct GuessTransform {
    zero_offset: f64,
    scale: f64,
    conjugate: bool,
    jitter: bool,
}

/// `EquationSystemSolver.buildGuessTransforms`, in the Java order: the 20
/// uniform transforms (conjugate × scale × zero-offset), then the 6
/// symmetry-breaking jitter variants.
fn guess_transforms() -> Vec<GuessTransform> {
    let mut transforms = Vec::with_capacity(26);
    for conjugate in [false, true] {
        for scale in [1.0, 1.0e-2, 1.0e-4, 1.0e2, 1.0e4] {
            for zero_offset in [1.0, -1.0] {
                transforms.push(GuessTransform {
                    zero_offset,
                    scale,
                    conjugate,
                    jitter: false,
                });
            }
        }
    }
    // Symmetry-breaking variants last: a system symmetric under a variable
    // exchange (x <-> y) traps every symmetric iteration on the invariant
    // manifold (identical Jacobian columns there), and the uniform transforms
    // above preserve that symmetry. Staggering each variable by its position
    // in the block is deterministic and leaves the manifold.
    for scale in [1.0, 1.0e-2, 1.0e2] {
        for zero_offset in [1.0, -1.0] {
            transforms.push(GuessTransform {
                zero_offset,
                scale,
                conjugate: false,
                jitter: true,
            });
        }
    }
    transforms
}

/// The initial guess a variable restarts from —
/// `EquationSystemSolver.initialGuess` without the warm-start map (no warm
/// starts in this port yet): the spec's guess clamped into its bounds, or
/// [`DEFAULT_GUESS`].
fn initial_guess(name: &str, specs: &BTreeMap<String, VarSpec>) -> f64 {
    specs
        .get(name)
        .map(|spec| spec.initial())
        .unwrap_or(DEFAULT_GUESS)
}

/// `EquationSystemSolver.applyTransform`: rewrite every block variable's value
/// from its *initial* guess (not the stalled iterate) through the transform,
/// clamped into the variable's bounds.
fn apply_transform(
    block: &Block,
    transform: &GuessTransform,
    values: &mut Scope,
    specs: &BTreeMap<String, VarSpec>,
) {
    for (position, name) in block.variables.iter().enumerate() {
        let base = initial_guess(name, specs);
        let mut guess = if base == 0.0 {
            transform.zero_offset
        } else {
            base * transform.scale
        };
        if transform.conjugate && name.ends_with("_i") {
            guess = -guess;
        }
        if transform.jitter {
            // Java: `guess *= 1.0 + 0.07 * ++position` — 1-based stagger.
            guess *= 1.0 + 0.07 * (position + 1) as f64;
        }
        if let Some(spec) = specs.get(name) {
            guess = guess.clamp(spec.lower, spec.upper);
        }
        values.insert(name.clone(), guess);
    }
}

/// **Ladder rung 1** — `EquationSystemSolver.retryWithTransformedGuesses`:
/// retry the failed block alone from each transformed start, with the
/// iteration-capped retry settings. Returns the iterations of the first
/// attempt that converges; on total failure restores the block's variables to
/// their unmodified initial guesses (so later rungs — and the failure
/// diagnostics — start from the guesses, exactly as the Java does) and
/// returns `None`.
#[allow(clippy::too_many_arguments)]
fn retry_with_transformed_guesses(
    index: usize,
    block: &Block,
    equations: &[Equation],
    values: &mut Scope,
    settings: &SolverSettings,
    specs: &BTreeMap<String, VarSpec>,
    defs: &Definitions,
) -> Option<usize> {
    let relaxed = retry_settings(settings);
    for transform in guess_transforms() {
        apply_transform(block, &transform, values, specs);
        if let Ok(iterations) = solve_block(index, block, equations, values, &relaxed, specs, defs)
        {
            return Some(iterations);
        }
    }
    for name in &block.variables {
        values.insert(name.clone(), initial_guess(name, specs));
    }
    None
}

/// True if either side of the equation contains a CoolProp `prop$` call —
/// `EquationSystemSolver.usesPropertyCall`. The recursion mirrors the Java
/// `exprUsesPropertyCall` exactly: `Call` (name prefix, then arguments),
/// `BinOp`, `Neg`; every other node answers false.
fn uses_property_call(equation: &Equation) -> bool {
    expr_uses_property_call(&equation.lhs) || expr_uses_property_call(&equation.rhs)
}

fn expr_uses_property_call(expr: &Expr) -> bool {
    match expr {
        Expr::Call { function, args } => {
            function.starts_with("prop$") || args.iter().any(expr_uses_property_call)
        }
        Expr::BinOp { left, right, .. } => {
            expr_uses_property_call(left) || expr_uses_property_call(right)
        }
        Expr::Neg(operand) => expr_uses_property_call(operand),
        _ => false,
    }
}

/// **Ladder rung 2** — `EquationSystemSolver.tryUnivariateBracketingSolve`:
/// a bracketing root-find for a one-equation, one-unknown block that Newton
/// could not solve. Newton needs a non-zero gradient; an implicit property
/// inversion crossing the two-phase dome is flat there (`dT/dh ≈ 0`) yet the
/// overall residual is monotonic and sign-changing (§8.7), so a sign-bracketed
/// bisection crosses the plateau where Newton stalls. Scoped to property
/// inversions (the `prop$` gate) exactly as the Java is — for ordinary algebra
/// a bracketing rescue would bypass the user's iteration-limit stop criterion
/// and could pick a different root than Newton's basin — which makes this rung
/// inert until CoolProp lands, but ready. The root is committed only if it
/// drives the residual within [`BRACKET_RESIDUAL_TOL`], so a wrong root can
/// never be silently accepted. Returns the work done on success, `None` when
/// no valid bracket/root was found (with the variable restored).
fn try_univariate_bracketing_solve(
    block: &Block,
    equations: &[Equation],
    values: &mut Scope,
    specs: &BTreeMap<String, VarSpec>,
    defs: &Definitions,
) -> Option<usize> {
    if block.variables.len() != 1 || block.equations.len() != 1 {
        return None;
    }
    let equation = equations.get(block.equations[0])?;
    if !uses_property_call(equation) {
        return None;
    }
    let name = &block.variables[0];
    let ctx = EvalContext { defs: Some(defs) };
    let (lower, upper) = specs
        .get(name)
        .map(|spec| (spec.lower, spec.upper))
        .unwrap_or((f64::NEG_INFINITY, f64::INFINITY));

    let saved = values.get(name).copied().unwrap_or(DEFAULT_GUESS);
    let x0 = if saved.is_finite() {
        saved
    } else {
        specs
            .get(name)
            .map(|spec| spec.guess)
            .filter(|guess| guess.is_finite())
            .unwrap_or(1.0)
    }
    .clamp(lower, upper);

    // Residual of the single equation with var = x; NaN inside invalid regions.
    let f = |x: f64, values: &mut Scope| -> f64 {
        values.insert(name.clone(), x);
        match (
            eval_with(&equation.lhs, values, ctx),
            eval_with(&equation.rhs, values, ctx),
        ) {
            (Ok(lhs), Ok(rhs)) => lhs - rhs,
            _ => f64::NAN,
        }
    };

    // Sample x0 plus geometrically growing symmetric offsets (clamped to the
    // box); keep only finite (in-range) evaluations. (The Java loop also
    // checks its elapsed-time deadline here; wasm32 has no clock, so the
    // sample count — ~69 evaluations — is the budget.)
    let magnitude = x0.abs().max(1.0);
    let mut samples: Vec<(f64, f64)> = Vec::new();
    let record = |x: f64, samples: &mut Vec<(f64, f64)>, values: &mut Scope| {
        if samples.iter().any(|&(seen, _)| seen == x) {
            return;
        }
        let value = f(x, values);
        if value.is_finite() {
            samples.push((x, value));
        }
    };
    record(x0, &mut samples, values);
    let mut multiplier = 0.125;
    while multiplier <= 1.0e9 {
        let step = magnitude * multiplier;
        record((x0 + step).clamp(lower, upper), &mut samples, values);
        record((x0 - step).clamp(lower, upper), &mut samples, values);
        multiplier *= 2.0;
    }
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));

    // Pick the adjacent finite-sample pair straddling zero whose midpoint is
    // nearest x0 (bias toward the local root).
    let mut bracket: Option<(f64, f64)> = None;
    let mut best_distance = f64::INFINITY;
    for pair in samples.windows(2) {
        let (xa, fa) = pair[0];
        let (xb, fb) = pair[1];
        if fa * fb < 0.0 {
            let distance = (0.5 * (xa + xb) - x0).abs();
            if distance < best_distance {
                best_distance = distance;
                bracket = Some((xa, xb));
            }
        }
    }
    let Some((mut a, mut b)) = bracket else {
        values.insert(name.clone(), saved);
        return None;
    };

    // Bisection on the bracket to a tight relative width.
    let mut fa = f(a, values);
    let mut iterations = 0usize;
    while iterations < 200 {
        iterations += 1;
        let c = 0.5 * (a + b);
        let fc = f(c, values);
        if !fc.is_finite() {
            break;
        }
        if fc == 0.0 || (b - a) <= 1.0e-12 * c.abs().max(1.0) {
            a = c;
            b = c;
            break;
        }
        if fa * fc < 0.0 {
            b = c;
        } else {
            a = c;
            fa = fc;
        }
    }

    // Validate the root against the residual before committing.
    let root = 0.5 * (a + b);
    values.insert(name.clone(), root);
    match (
        eval_with(&equation.lhs, values, ctx),
        eval_with(&equation.rhs, values, ctx),
    ) {
        (Ok(lhs), Ok(rhs)) => {
            let residual = lhs - rhs;
            let scale = lhs.abs().max(1.0);
            if residual.is_finite() && residual.abs() / scale <= BRACKET_RESIDUAL_TOL {
                return Some(iterations + 1);
            }
            values.insert(name.clone(), saved);
            None
        }
        _ => {
            values.insert(name.clone(), saved);
            None
        }
    }
}

/// **Ladder rung 3** — `EquationSystemSolver.tryMergeBidirectional`: merge the
/// failed block with blocks that share variable dependencies, scanning forward
/// then backward, skipping blocks a previous merge already solved. Returns the
/// merged block and the indices it swallowed, or `None` when nothing merges.
fn try_merge_bidirectional(
    blocks: &[Block],
    failed_index: usize,
    equations: &[Equation],
    skip: &HashSet<usize>,
) -> Option<(Block, Vec<usize>)> {
    let failed = &blocks[failed_index];
    let mut involved_vars: HashSet<String> = HashSet::new();
    let mut merged_equations = failed.equations.clone();
    let mut merged_vars = failed.variables.clone();
    let mut merged_var_set: HashSet<String> = failed.variables.iter().cloned().collect();
    for &equation_index in &failed.equations {
        if let Some(equation) = equations.get(equation_index) {
            involved_vars.extend(equation.variables());
        }
    }
    let mut merged_indices = Vec::new();

    let consider = |candidate_index: usize,
                    merged_equations: &mut Vec<usize>,
                    merged_vars: &mut Vec<String>,
                    merged_var_set: &mut HashSet<String>,
                    involved_vars: &mut HashSet<String>,
                    merged_indices: &mut Vec<usize>| {
        let candidate = &blocks[candidate_index];
        if !should_merge(candidate, equations, involved_vars, merged_var_set) {
            return;
        }
        // Java addBlock: absorb the candidate's equations, variables, and
        // every variable its equations reference.
        merged_equations.extend(candidate.equations.iter().copied());
        merged_vars.extend(candidate.variables.iter().cloned());
        merged_var_set.extend(candidate.variables.iter().cloned());
        for &equation_index in &candidate.equations {
            if let Some(equation) = equations.get(equation_index) {
                involved_vars.extend(equation.variables());
            }
        }
        merged_indices.push(candidate_index);
    };

    // Scan forward: blocks whose variables are referenced by the merged block,
    // or whose equations reference merged variables.
    for candidate_index in failed_index + 1..blocks.len() {
        if skip.contains(&candidate_index) {
            continue;
        }
        consider(
            candidate_index,
            &mut merged_equations,
            &mut merged_vars,
            &mut merged_var_set,
            &mut involved_vars,
            &mut merged_indices,
        );
    }
    // Scan backward: earlier blocks whose variables the failed block's
    // equations reference, or that reference our variables.
    for candidate_index in (0..failed_index).rev() {
        if skip.contains(&candidate_index) {
            continue;
        }
        consider(
            candidate_index,
            &mut merged_equations,
            &mut merged_vars,
            &mut merged_var_set,
            &mut involved_vars,
            &mut merged_indices,
        );
    }

    if merged_indices.is_empty() {
        return None;
    }
    Some((
        Block {
            equations: merged_equations,
            variables: merged_vars,
        },
        merged_indices,
    ))
}

/// `EquationSystemSolver.shouldMerge`: the candidate determines a variable the
/// merged block's equations reference, or its equations reference a merged
/// variable.
fn should_merge(
    candidate: &Block,
    equations: &[Equation],
    involved_vars: &HashSet<String>,
    merged_var_set: &HashSet<String>,
) -> bool {
    if candidate
        .variables
        .iter()
        .any(|name| involved_vars.contains(name))
    {
        return true;
    }
    candidate.equations.iter().any(|&equation_index| {
        equations.get(equation_index).is_some_and(|equation| {
            equation
                .variables()
                .iter()
                .any(|v| merged_var_set.contains(v))
        })
    })
}

/// Solve one block through the whole Java retry ladder — the port of
/// `EquationSystemSolver.solveBlockWithFallback`, rung by rung:
///
/// 1. the plain Newton solve (`config.newton().solveBlock`);
/// 2. on failure, **transformed-guess retries** — local and cheap, keeping all
///    other blocks' solutions intact (`retryWithTransformedGuesses`);
/// 3. then the **univariate bracketing rescue** for one-unknown property
///    inversions (`tryUnivariateBracketingSolve`);
/// 4. then **bidirectional block merging**: re-solve the combined system from
///    scratch — all merged variables reset to their initial guesses, because
///    previously solved blocks may carry incorrect values from the
///    rank-deficient/least-squares path — and mark the swallowed blocks
///    solved (`tryMergeBidirectional` + `skipIndices`); a merge that itself
///    fails to converge propagates *its* error, while an impossible merge
///    rethrows the original one;
/// 5. finally a best-effort **polish pass** over whatever was solved
///    (`polishSettings`) — its iterations count only when it converges, and
///    its failure is ignored (the main solution is still valid), exactly like
///    the Java `catch (SolverException ignored)`.
#[allow(clippy::too_many_arguments)]
fn solve_block_with_fallback(
    index: usize,
    blocks: &[Block],
    equations: &[Equation],
    values: &mut Scope,
    settings: &SolverSettings,
    specs: &BTreeMap<String, VarSpec>,
    defs: &Definitions,
    skip: &mut HashSet<usize>,
) -> Result<usize> {
    let block = &blocks[index];
    let mut actual_solved: Option<Block> = None; // None = the original block
    let mut iterations = 0usize;

    match solve_block(index, block, equations, values, settings, specs, defs) {
        Ok(count) => iterations += count,
        Err(first_error) => {
            if let Some(count) = retry_with_transformed_guesses(
                index, block, equations, values, settings, specs, defs,
            ) {
                iterations += count;
            } else if let Some(count) =
                try_univariate_bracketing_solve(block, equations, values, specs, defs)
            {
                iterations += count;
            } else {
                let merged = try_merge_bidirectional(blocks, index, equations, skip)
                    .filter(|(merged, _)| merged.variables.len() > block.variables.len());
                match merged {
                    Some((merged, merged_indices)) => {
                        // Reset ALL variables in the merged block to initial
                        // guesses: previously solved blocks may have incorrect
                        // values from the rank-deficient fallback.
                        for name in &merged.variables {
                            values.insert(name.clone(), initial_guess(name, specs));
                        }
                        // A merge that fails to converge propagates its own
                        // error (Java: the uncaught `config.newton().solveBlock`
                        // inside the catch block).
                        iterations +=
                            solve_block(index, &merged, equations, values, settings, specs, defs)?;
                        skip.extend(merged_indices);
                        actual_solved = Some(merged);
                    }
                    None => return Err(first_error),
                }
            }
        }
    }

    // Polish pass — best-effort; the main solution is still valid if it fails.
    let polish = polish_settings(settings);
    let polished_block = actual_solved.as_ref().unwrap_or(block);
    if let Ok(count) = solve_block(
        index,
        polished_block,
        equations,
        values,
        &polish,
        specs,
        defs,
    ) {
        iterations += count;
    }
    Ok(iterations)
}

// ---------------------------------------------------------------------------
// The block loop, shared by the main solve and the Integral driver
// ---------------------------------------------------------------------------

/// What a completed inner solve leaves behind — the Java
/// `EquationSystemSolver.InnerSolve` minus its `blocks`, which no caller of
/// [`solve_equation_list`] needs.
struct InnerSolve {
    /// Every variable of the system at the solution, plus the built-in
    /// constants that were seeded so the evaluator could read them.
    values: Scope,
    iterations: usize,
}

/// A block that gave up, with the index it happened at and the work done
/// before it (the Java `SolverException.FailureState`, minus the copies of the
/// blocks and values the caller already holds).
struct BlockLoopFailure {
    error: FreesError,
    failed_block_index: usize,
    iterations: usize,
}

/// Solve every block in order through the retry ladder, writing each block's
/// answer into `values` as it goes (that *is* the "feed knowns forward"
/// mechanism). The Java `solveEquationList`'s block loop, factored out so the
/// pinned subsystems the `Integral` stepper solves run through exactly the
/// same path as the top-level system.
fn run_blocks(
    blocks: &[Block],
    equations: &[Equation],
    values: &mut Scope,
    settings: &SolverSettings,
    specs: &BTreeMap<String, VarSpec>,
    defs: &Definitions,
) -> std::result::Result<usize, BlockLoopFailure> {
    let mut iterations = 0usize;
    // Blocks a merge rescue already solved (Java's `skipIndices`).
    let mut skip: HashSet<usize> = HashSet::new();
    for index in 0..blocks.len() {
        if skip.contains(&index) {
            continue;
        }
        match solve_block_with_fallback(
            index, blocks, equations, values, settings, specs, defs, &mut skip,
        ) {
            Ok(block_iterations) => iterations += block_iterations,
            Err(error) => {
                return Err(BlockLoopFailure {
                    error,
                    failed_block_index: index,
                    iterations,
                })
            }
        }
    }
    Ok(iterations)
}

/// Block and solve a standalone equation list, seeded from `warm_start` where
/// it has a value for a variable and from `specs` otherwise — the Java
/// `EquationSystemSolver.solveEquationList` (`initialGuess(name, specs,
/// warmStart)`).
///
/// Only the `Integral` driver uses this. [`solve_with`] drives
/// [`block_system`] and [`run_blocks`] itself because it needs the blocking
/// report, the specs and the diagnostics for its own reporting.
fn solve_equation_list(
    equations: &[Equation],
    settings: &SolverSettings,
    specs: &BTreeMap<String, VarSpec>,
    defs: &Definitions,
    warm_start: Option<&Scope>,
) -> Result<InnerSolve> {
    let (constants, knowns) = builtin_constants(equations);
    let report = block_system(equations, &knowns)?;

    let mut values: Scope = HashMap::new();
    values.extend(constants);
    for name in unknowns(equations, &knowns) {
        let guess = warm_start
            .and_then(|warm| warm.get(&name).copied())
            .unwrap_or_else(|| initial_guess(&name, specs));
        values.insert(name, guess);
    }

    let iterations = run_blocks(
        &report.blocks,
        equations,
        &mut values,
        settings,
        specs,
        defs,
    )
    .map_err(|failure| failure.error)?;
    Ok(InnerSolve { values, iterations })
}

// ---------------------------------------------------------------------------
// The Integral pass — port of EquationSystemSolver.solveWithIntegrals
// ---------------------------------------------------------------------------

/// Every `Integral` equation of the system, or an empty list when the document
/// mentions none.
///
/// The Java `EquationSystemSolver.findIntegrals`: detection runs on the raw
/// equations (before complex expansion), and complex mode is refused outright
/// rather than producing an expansion the quadrature cannot drive.
fn find_integrals(
    equations: &[Equation],
    defs: &Definitions,
    complex_mode: bool,
) -> Result<Vec<IntegralEquation>> {
    let mentions = equations.iter().any(|equation| {
        crate::integral::mentions_integral(&equation.lhs)
            || crate::integral::mentions_integral(&equation.rhs)
    });
    if !mentions {
        return Ok(Vec::new());
    }
    if complex_mode {
        return Err(FreesError::solver(
            "Integral is not supported in complex mode.",
        ));
    }
    crate::integral::extract(equations, defs)
}

/// Lower every `Integral` equation into the equation list the solver actually
/// blocks — the Java `solveWithIntegrals` plus `appendIntegralEquations`.
///
/// * A **variable-limit** integral (`F = Integral(2*t, t, 0, b)`) becomes the
///   inlined quadrature equation, which the evaluator recomputes at every
///   Newton residual, plus — once per integration variable — a pin
///   `t = <upper expression>`.
/// * A **constant-limit** integral is driven *here*, before the main solve:
///   [`crate::integral::integrate`] sweeps `t` from `a` to `b`, re-solving the
///   ordinary subsystem with `t` and the running total pinned at every
///   quadrature point, and the result becomes a numeric equation
///   `F = <value>`, with `t` pinned at the upper limit.
///
/// Those pins are what make an integral document square: without them
/// `F = Integral(t^2, t, 0, 1)` is one equation in two unknowns. They are also
/// why the integration variable **survives as a result variable** sitting at
/// the upper limit, which is the parent engine's documented behaviour.
///
/// Returns the lowered equations and the Newton iterations the sweeps
/// consumed (the Java `IntegrationState.iterations`, which is added to the
/// reported total).
fn lower_integrals(
    equations: &[Equation],
    integrals: &[IntegralEquation],
    settings: &SolverSettings,
    specs: &BTreeMap<String, VarSpec>,
    defs: &Definitions,
) -> Result<(Vec<Equation>, usize)> {
    let ordinary = crate::integral::ordinary_equations(equations, integrals);
    let mut lowered = ordinary.clone();
    let mut pinned_integration_vars: BTreeSet<String> = BTreeSet::new();
    // The Java `IntegrationState` is built once per document and shared by
    // every integral, so a later sweep warm-starts from the last subsystem
    // solve of an earlier one.
    let mut warm_start: Option<Scope> = None;
    let mut stepping_iterations = 0usize;

    for ie in integrals {
        if !ie.constant_limits() {
            lowered.push(crate::integral::inlined_equation(ie, &ordinary)?);
            if pinned_integration_vars.insert(ie.integration_var.clone()) {
                lowered.push(Equation::new(
                    Expr::Var(ie.integration_var.clone()),
                    ie.upper_expr.clone(),
                    format!(
                        "{} = upper limit of {}",
                        ie.integration_var, ie.original.source_text
                    ),
                ));
            }
            continue;
        }

        let value = crate::integral::integrate(
            |t, running_total| {
                let solved = solve_pinned(
                    &ordinary,
                    &[
                        (ie.integration_var.clone(), t),
                        (ie.result_var.clone(), running_total),
                    ],
                    settings,
                    specs,
                    defs,
                    warm_start.as_ref(),
                )
                .map_err(|err| integral_point_failure(ie, t, &err))?;
                stepping_iterations += solved.iterations;
                let point = eval_with(
                    &ie.integrand,
                    &solved.values,
                    EvalContext { defs: Some(defs) },
                )
                .map_err(|err| integral_point_failure(ie, t, &err));
                warm_start = Some(solved.values);
                point
            },
            ie.lower(),
            ie.upper(),
            ie.fixed_step,
        )?;

        if pinned_integration_vars.insert(ie.integration_var.clone()) {
            lowered.push(Equation::new(
                Expr::Var(ie.integration_var.clone()),
                Expr::num(ie.upper()),
                format!("{} = {}", ie.integration_var, ie.upper()),
            ));
        }
        lowered.push(Equation::new(
            Expr::Var(ie.result_var.clone()),
            Expr::num(value),
            ie.original.source_text.clone(),
        ));
    }
    Ok((lowered, stepping_iterations))
}

/// Solve `ordinary` with a set of variables pinned to fixed values, seeded
/// from `warm_start`. Each pinned variable is appended as a `var = value`
/// equation (insertion order preserved) so the existing Newton/Tarjan pipeline
/// treats it as a constant for this solve.
///
/// The Java `EquationSystemSolver.solvePinned` — the one place the
/// "solve the rest of the system with the integration variable fixed"
/// behaviour lives.
fn solve_pinned(
    ordinary: &[Equation],
    pinned: &[(String, f64)],
    settings: &SolverSettings,
    specs: &BTreeMap<String, VarSpec>,
    defs: &Definitions,
    warm_start: Option<&Scope>,
) -> Result<InnerSolve> {
    let mut subsystem: Vec<Equation> = ordinary.to_vec();
    for (name, value) in pinned {
        subsystem.push(Equation::new(
            Expr::Var(name.clone()),
            Expr::num(*value),
            format!("{name} = {value}"),
        ));
    }
    solve_equation_list(&subsystem, settings, specs, defs, warm_start)
}

/// Name the quadrature point a subsystem solve or an integrand evaluation gave
/// up at — the Java `integrandAt`'s `catch` wrapper.
fn integral_point_failure(ie: &IntegralEquation, t: f64, err: &FreesError) -> FreesError {
    FreesError::solver(format!(
        "While evaluating Integral at {} = {t}: {}",
        ie.integration_var,
        err.to_string_message()
    ))
}

/// Prefix a block failure with the equations it came from, quoted verbatim.
///
/// The parent engine's rule is that diagnostics are source-mapped and quote the
/// user's own line; a bare "did not converge" from a 4×4 block is unactionable.
fn annotate(err: FreesError, index: usize, equations: &[&Equation]) -> FreesError {
    let quoted: Vec<&str> = equations.iter().map(|e| e.source_text.as_str()).collect();
    let plural = if quoted.len() == 1 { "" } else { "s" };
    let message = format!(
        "Block {} ({} equation{}) failed: {}\n  {}",
        index + 1,
        quoted.len(),
        plural,
        err.to_string_message(),
        quoted.join("\n  ")
    );
    match err {
        FreesError::Parse { span, .. } => FreesError::Parse { message, span },
        _ => FreesError::solver(message),
    }
}

/// Warn about any solved value that left the bounds its `GUESS` declared.
///
/// Since [`crate::solver::newton::newton_solve`] enforces bounds at every
/// probe (the three Java
/// `Math.clamp` sites), a bounded solve can no longer end outside its box and
/// this warning should be unreachable for bounded variables. It is kept as a
/// safety net — a future warm-start path, or a spec produced outside
/// `variable_specs`, must still never return a silently out-of-range answer.
fn check_bounds(
    specs: &BTreeMap<String, VarSpec>,
    values: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (name, spec) in specs {
        if !spec.is_bounded() {
            continue;
        }
        let Some(&value) = values.get(name) else {
            continue;
        };
        if value < spec.lower || value > spec.upper {
            diagnostics.push(Diagnostic::warning(format!(
                "`{name}` solved to {value}, outside the GUESS bounds \
                 [{}, {}]; bounds are not enforced during iteration",
                spec.lower, spec.upper
            )));
        }
    }
}

// ---------------------------------------------------------------------------
// Small helper on the error type
// ---------------------------------------------------------------------------

impl FreesError {
    /// The message without the `Display` impl's `"<kind> error: "` prefix.
    ///
    /// Re-wrapping an error would otherwise stutter
    /// (`solver error: Block 1 failed: solver error: …`).
    pub fn to_string_message(&self) -> String {
        match self {
            FreesError::Parse { message, .. }
            | FreesError::Solver { message }
            | FreesError::Property { message }
            | FreesError::Evaluation { message } => message.clone(),
            FreesError::UnknownUnit { unit } => format!("unknown unit: {unit}"),
        }
    }
}

/// The variables the document mentions that are neither unknowns nor built-in
/// constants — currently always empty, kept as the hook the component expander
/// will need. Exposed so callers can assert the invariant.
pub fn known_constants(equations: &[Equation]) -> BTreeMap<String, f64> {
    builtin_constants(equations).0
}

/// All variable names in a parsed document, sorted — unknowns and constants
/// alike. Handy for editors that want to autocomplete against the document.
pub fn document_variables(doc: &Document) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for equation in doc.equations() {
        out.extend(equation.variables());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solved(source: &str) -> Solution {
        solve(source, &SolverSettings::default())
            .unwrap_or_else(|err| panic!("solve failed for {source:?}: {err}"))
    }

    fn value(solution: &Solution, name: &str) -> f64 {
        *solution
            .values
            .get(name)
            .unwrap_or_else(|| panic!("no value for {name}: {:?}", solution.values))
    }

    fn assert_close(actual: f64, expected: f64) {
        let tolerance = 1e-9 * expected.abs().max(1.0);
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn a_single_assignment_solves() {
        let solution = solved("a = 2");
        assert_close(value(&solution, "a"), 2.0);
        assert_eq!(solution.blocks.len(), 1);
        assert!(solution.blocks[0].is_scalar());
    }

    #[test]
    fn a_chain_solves_in_dependency_order_not_source_order() {
        let solution = solved("c = b + 1\nb = a * 3\na = 2\n");
        assert_close(value(&solution, "a"), 2.0);
        assert_close(value(&solution, "b"), 6.0);
        assert_close(value(&solution, "c"), 7.0);

        let order: Vec<&str> = solution
            .blocks
            .iter()
            .map(|b| b.variables[0].as_str())
            .collect();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn a_simultaneous_pair_is_one_block() {
        let solution = solved("u + v = 10\nu - v = 2\n");
        assert_close(value(&solution, "u"), 6.0);
        assert_close(value(&solution, "v"), 4.0);
        assert_eq!(solution.blocks.len(), 1);
        assert_eq!(solution.blocks[0].variables, vec!["u", "v"]);
    }

    #[test]
    fn built_in_constants_are_knowns_not_unknowns() {
        let solution = solved("a = pi#\nb = R#\nc = g#\n");
        assert_close(value(&solution, "a"), std::f64::consts::PI);
        assert_close(value(&solution, "b"), 8.314_462_618);
        assert_close(value(&solution, "c"), 9.806_65);
        // Three equations, three unknowns — the constants must not have been
        // counted as free variables.
        assert_eq!(solution.blocks.len(), 3);
    }

    #[test]
    fn guess_directives_seed_the_iterate() {
        // x^2 = 9 has two roots; the guess picks which one is found.
        assert_close(value(&solved("GUESS x = 3\nx ^ 2 = 9\n"), "x"), 3.0);
        assert_close(value(&solved("GUESS x = -3\nx ^ 2 = 9\n"), "x"), -3.0);
    }

    #[test]
    fn a_guess_for_an_unknown_name_is_a_warning_not_a_failure() {
        let solution = solved("GUESS zzz = 3\nx = 1\n");
        assert_close(value(&solution, "x"), 1.0);
        assert_eq!(solution.diagnostics.len(), 1);
        assert!(solution.diagnostics[0].message.contains("zzz"));
        assert_eq!(
            solution.diagnostics[0].severity,
            crate::diag::Severity::Warning
        );
    }

    #[test]
    fn guess_bounds_clamp_the_starting_point() {
        let mut spec = VarSpec::default();
        apply_guess(
            &mut spec,
            &GuessDirective {
                name: "x".into(),
                guess: Some(100.0),
                lower: Some(0.0),
                upper: Some(10.0),
            },
        );
        assert_eq!(spec.initial(), 10.0);
    }

    #[test]
    fn reversed_guess_bounds_are_repaired_not_panicked() {
        let mut spec = VarSpec::default();
        apply_guess(
            &mut spec,
            &GuessDirective {
                name: "x".into(),
                guess: Some(5.0),
                lower: Some(10.0),
                upper: Some(0.0),
            },
        );
        assert_eq!(spec.lower, 0.0);
        assert_eq!(spec.upper, 10.0);
        assert_eq!(spec.initial(), 5.0);
    }

    #[test]
    fn units_convert_to_si_at_parse_time() {
        let solution = solved("P = 140 [kPa]\nQ = P * 2\n");
        assert_close(value(&solution, "p"), 140_000.0);
        assert_close(value(&solution, "q"), 280_000.0);
    }

    #[test]
    fn an_empty_document_is_a_solver_error() {
        let err = solve("{ only a comment }", &SolverSettings::default()).unwrap_err();
        assert_eq!(err, FreesError::solver("No equations to solve."));
    }

    #[test]
    fn an_overdetermined_document_names_the_redundant_relation() {
        let err = solve("z = 1\nz = 2\n", &SolverSettings::default()).unwrap_err();
        let message = err.to_string_message();
        assert!(matches!(err.error, FreesError::Solver { .. }), "{err:?}");
        assert!(message.contains("2 equations and 1 variables"), "{message}");
        assert!(message.contains("overspecified"), "{message}");
    }

    #[test]
    fn an_underdetermined_document_names_the_free_quantity() {
        let err = solve("m + n = 5\n", &SolverSettings::default()).unwrap_err();
        let message = err.to_string_message();
        assert!(message.contains("underspecified"), "{message}");
        assert!(message.contains('n'), "{message}");
    }

    #[test]
    fn an_unsupported_block_is_refused_by_name() {
        let err = solve("PLOT 'x'\n  kind = xy\nEND\n", &SolverSettings::default()).unwrap_err();
        assert!(matches!(err.error, FreesError::Parse { .. }), "{err:?}");
        assert!(err.to_string_message().contains("PLOT"));
    }

    /// The three component forms after Phase 6 wired the expander in. The
    /// capability gate that used to refuse all three is gone; each now gets the
    /// answer the reference engine gives, and each of these three messages was
    /// checked against the Java oracle character for character (modulo the
    /// port-wide `source_text` convention, which keeps the user's whitespace
    /// where ANTLR's `getText()` drops it).
    #[test]
    fn the_three_component_forms_answer_like_the_reference_engine() {
        // A template nobody instantiates contributes nothing, so the document is
        // empty — a *solver* verdict, not a parse one.
        let err = solve(
            "COMPONENT pump(in, out)\n  out.P = in.P\nEND\n",
            &SolverSettings::default(),
        )
        .unwrap_err();
        assert!(matches!(err.error, FreesError::Solver { .. }), "{err:?}");
        assert_eq!(err.to_string_message(), "No equations to solve.");

        // An instantiation missing a required parameter is refused by name at
        // expansion time — the library ships no defaults for physical inputs.
        let err = solve("Pump P1(s1, s2)\nx = 1\n", &SolverSettings::default()).unwrap_err();
        assert!(matches!(err.error, FreesError::Parse { .. }), "{err:?}");
        assert_eq!(
            err.to_string_message(),
            "Component 'p1' (pump): parameter 'eta' has no value \
             (give it a default or pass eta=value)."
        );

        // A `connect` naming something that is neither an instance port nor a
        // stream is refused, quoting the declaration.
        let err = solve("connect(a.out, b.in)\nx = 1\n", &SolverSettings::default()).unwrap_err();
        assert!(matches!(err.error, FreesError::Parse { .. }), "{err:?}");
        assert!(
            err.to_string_message().starts_with(
                "connect(...): 'a.out' is not a port (instance.port) or a stream name."
            ),
            "{err:?}"
        );

        // …and a document with no component layer never touches the expander.
        assert!(solve("x = 1\n", &SolverSettings::default()).is_ok());
    }

    /// The end-to-end shape the gate used to block: a built-in instantiated,
    /// wired, and solved through the ordinary Newton/Tarjan path.
    #[test]
    fn a_component_network_expands_and_solves() {
        let solution = solve(
            "Resistor R1(n1, n2, R = 10)\nGround G1(n2)\nn1.V = 12\n",
            &SolverSettings::default(),
        )
        .expect("the network solves");
        assert_eq!(solution.values["n2$v"], 0.0);
        assert_eq!(solution.values["n1$i"], 1.2);
        // Port members carry the display spelling the expander minted…
        assert_eq!(solution.display_names["n1$i"], "n1.i");
        // …and the SI unit their physical domain fixes, which nothing in the
        // document could have derived.
        assert_eq!(solution.inferred_units["n1$i"], "A");
        // The datasheet payload rides out on the solution.
        assert_eq!(solution.component_instances.len(), 2);
        assert_eq!(solution.component_instances[0].type_name, "resistor");
    }

    /// A storage network with no `DYNAMIC` block solves its steady operating
    /// point (`der(X) = rhs` → `rhs = 0`) and says so in a diagnostic.
    #[test]
    fn a_storage_network_without_a_dynamic_block_solves_steady() {
        let solution = solve(
            "TorqueSource     TQ(T = 5)\n\
             RotationalDamper DP(c = 0.25)\n\
             Inertia          IN(J = 0.4, w0 = 0)\n\
             MechGround       G1()\n\
             MechGround       G2()\n\
             connect(TQ.a, IN.port, DP.a)\n\
             connect(TQ.b, G1.port)\n\
             connect(DP.b, G2.port)\n",
            &SolverSettings::default(),
        )
        .expect("the steady network solves");
        // der(w) = tau/J with tau summing to zero at the node: the damper takes
        // the whole 5 N.m, so w = 5 / 0.25 = 20 rad/s.
        assert_eq!(solution.values["in$port$w"], 20.0);
        assert!(
            solution
                .diagnostics
                .iter()
                .any(|d| d.message.contains("steady operating point")),
            "{:?}",
            solution.diagnostics
        );
    }

    #[test]
    fn a_syntax_error_is_a_parse_error_with_a_span() {
        let err = solve("x = = 2\n", &SolverSettings::default()).unwrap_err();
        assert!(matches!(err.error, FreesError::Parse { .. }), "{err:?}");
        assert!(err.span().is_some());
    }

    #[test]
    fn check_reports_a_solvable_system_without_solving() {
        let report = check("x^2 + y^3 = 77\nx/y = 1.23456\n").unwrap();
        assert!(report.solvable);
        assert_eq!(report.equation_count, 2);
        assert_eq!(report.unknown_count, 2);
        assert_eq!(report.variables, vec!["x", "y"]);
        assert_eq!(
            report.message,
            "No syntax errors were detected. There are 2 equations and 2 variables."
        );
    }

    #[test]
    fn check_reports_structural_failure_without_erroring() {
        let report = check("m + n = 5\n").unwrap();
        assert!(!report.solvable);
        assert_eq!(report.equation_count, 1);
        assert_eq!(report.unknown_count, 2);
        assert!(
            report.message.contains("underspecified"),
            "{}",
            report.message
        );
    }

    #[test]
    fn check_reports_an_empty_document_as_unsolvable() {
        let report = check("{ nothing }").unwrap();
        assert!(!report.solvable);
        assert_eq!(report.equation_count, 0);
        assert_eq!(report.message, "No equations to solve.");
    }

    #[test]
    fn check_reports_a_syntax_error_as_data_not_an_err() {
        // Mirrors the Java 400-with-body: the report is the payload api.ts
        // parses, so a parse failure must come back as a report, not an Err.
        let report = check("x = = 2").unwrap();
        assert!(!report.solvable);
        assert_eq!(report.equation_count, 0);
        assert_eq!(report.unknown_count, 0);
        assert!(report.variables.is_empty());
        assert!(
            report.message.starts_with("Syntax error: "),
            "{}",
            report.message
        );
        assert_eq!(report.error_line, Some(1));
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].line, 1);
        assert!(report.errors[0].column >= 1);
    }

    #[test]
    fn check_error_line_is_the_line_of_the_broken_statement() {
        let report = check("a = 1\nb = 2\nc = = 3\n").unwrap();
        assert_eq!(report.error_line, Some(3));
        assert_eq!(report.errors[0].line, 3);
    }

    #[test]
    fn check_does_not_solve() {
        // `1/0` would be an evaluation error if this were solved; check must
        // not touch the evaluator.
        let report = check("x = 1 / 0\n").unwrap();
        assert!(report.solvable);
    }

    #[test]
    fn a_domain_error_at_the_initial_point_is_reported_verbatim() {
        let err = solve("x = 1 / 0\n", &SolverSettings::default()).unwrap_err();
        let message = err.to_string_message();
        assert!(message.contains("division by zero"), "{message}");
        // and it quotes the offending equation
        assert!(message.contains("x = 1 / 0"), "{message}");
    }

    #[test]
    fn an_unknown_function_is_reported_not_silently_nan() {
        let err = solve("x = nosuchfn(2)\n", &SolverSettings::default()).unwrap_err();
        assert!(
            err.to_string_message().contains("nosuchfn"),
            "{}",
            err.to_string_message()
        );
    }

    #[test]
    fn an_invalid_region_reached_mid_iteration_does_not_abort_the_solve() {
        // ln(x) = 0 → x = 1. Starting at the default guess 1.0 is fine, but the
        // Newton path for `ln(x) - 0` probes and can step towards x <= 0, where
        // `eval` raises rather than returning NaN. The engine must survive it.
        let solution = solved("ln(x) = 0\n");
        assert_close(value(&solution, "x"), 1.0);
    }

    #[test]
    fn iterations_are_summed_across_blocks() {
        // Each block's cost is added, so a two-block document costs strictly
        // more than either block alone.
        let one = solved("a = 2\n");
        let two = solved("a = 2\nb = a * 3\n");
        assert_eq!(one.iterations, 1);
        assert_eq!(two.blocks.len(), 2);
        assert!(two.iterations > one.iterations, "{}", two.iterations);
        assert!(
            two.iterations <= 4,
            "linear blocks should be cheap: {}",
            two.iterations
        );

        // A nonlinear block genuinely iterates.
        let nonlinear = solved("x ^ 3 - 2 * x - 5 = 0\n");
        assert!(nonlinear.iterations > 1, "{}", nonlinear.iterations);
        assert_close(value(&nonlinear, "x"), 2.094_551_481_542_326_6);
    }

    #[test]
    fn case_insensitive_names_collapse_to_one_variable() {
        let solution = solved("Tin = 300\nT_out = TIN * 2\nresult = t_Out + tin\n");
        assert_eq!(solution.values.len(), 3);
        assert_close(value(&solution, "tin"), 300.0);
        assert_close(value(&solution, "t_out"), 600.0);
        assert_close(value(&solution, "result"), 900.0);
    }

    #[test]
    fn for_block_bodies_are_flattened_into_the_system() {
        // `expand_document` unrolls FOR bodies per iteration with the loop
        // variable substituted (the Java `EquationParser.flatten` rule), so
        // the body must use the index — a constant body would correctly be
        // rejected as the same equation stated twice.
        let solution = solved("FOR i = 1 TO 2\n  a[i] = 5 * i\nEND\nb = a[1] + a[2]\n");
        assert_close(value(&solution, "a[1]"), 5.0);
        assert_close(value(&solution, "a[2]"), 10.0);
        assert_close(value(&solution, "b"), 15.0);
    }

    #[test]
    fn symbolic_statements_are_refused_rather_than_dropped() {
        let err = solve("SYMBOLIC s\nx = 1\n", &SolverSettings::default()).unwrap_err();
        assert!(err.to_string_message().contains("SYMBOLIC"), "{err}");
    }

    #[test]
    fn call_statements_are_refused_rather_than_dropped() {
        // The refusal comes from the `flatten_calls` pipeline stage: a CALL
        // to an unknown name is an error naming it (the Java flattenCallProc
        // behaviour), never a silently dropped statement.
        let err = solve("CALL mix(1, 2 : y)\nx = 1\n", &SolverSettings::default()).unwrap_err();
        let message = err.to_string_message();
        assert!(message.contains("mix"), "{message}");
    }

    #[test]
    fn a_block_failure_quotes_its_equations() {
        // exp(x) = -1 has no real root: the block cannot converge, and the
        // message must say which equation failed.
        let err = solve("exp(x) = -1\n", &SolverSettings::default()).unwrap_err();
        let message = err.to_string_message();
        assert!(message.contains("exp(x) = -1"), "{message}");
        assert!(message.starts_with("Block 1"), "{message}");
    }

    #[test]
    fn error_messages_do_not_stutter_their_kind_prefix() {
        let err = FreesError::solver("did not converge");
        assert_eq!(err.to_string(), "solver error: did not converge");
        assert_eq!(err.to_string_message(), "did not converge");
    }

    #[test]
    fn constants_fold_at_parse_time_like_the_java_ast_builder() {
        // `AstBuilder.visitVarAtom` substitutes built-in `#` constants as
        // numeric literals at parse time, so they never appear as document
        // variables and the knowns hook has nothing left to collect.
        let doc = parse_document("a = pi# + b\n").unwrap();
        let vars: Vec<String> = document_variables(&doc).into_iter().collect();
        assert_eq!(vars, vec!["a", "b"]);
        assert!(
            known_constants(&doc.equations().into_iter().cloned().collect::<Vec<_>>()).is_empty()
        );

        // The folded literal carries the raw SI unit string, grounding the
        // unit checker: v = g#*t infers v as m/s (m/s^2 times s).
        let g = parse_document("x = g#\n").unwrap();
        match &g.equations()[0].rhs {
            Expr::Num { value, unit, .. } => {
                assert!((value - 9.806_65).abs() < 1e-12);
                assert_eq!(unit.as_deref(), Some("m/s^2"));
            }
            other => panic!("g# should fold to a literal, got {other:?}"),
        }

        // Unknown `#` names stay variables, exactly like the Java lookup miss.
        let unknown = parse_document("y = zz#\n").unwrap();
        assert_eq!(
            document_variables(&unknown).into_iter().collect::<Vec<_>>(),
            vec!["y", "zz#"]
        );
    }

    #[test]
    fn constants_are_used_but_not_reported_as_variables() {
        let solution = solved("a = 2 * pi#\n");
        assert_close(value(&solution, "a"), 2.0 * std::f64::consts::PI);
        // `fixtures/golden/constants.json` lists only the document's own
        // variables — a folded constant is not a result row.
        assert_eq!(
            solution.values.keys().collect::<Vec<_>>(),
            vec![&"a".to_string()]
        );
    }

    /// Bounds are enforced now (the Java `Math.clamp` sites): a document whose
    /// equations force a value outside its declared box cannot solve — the
    /// whole retry ladder runs inside the box and exhausts, exactly as the
    /// Java engine fails this document. (It used to "solve" to 5.0 with a
    /// warning; that was ranked divergence #3 in `docs/status-phase1.md`.)
    #[test]
    fn a_solution_outside_the_guess_bounds_is_a_ladder_exhaustion_failure() {
        let failure = solve("GUESS x = 0.5 [0, 1]\nx = 5\n", &SolverSettings::default())
            .expect_err("x = 5 is unreachable inside [0, 1]");
        assert_eq!(failure.failed_block_index, Some(0));
        let partial = failure
            .partial
            .as_deref()
            .expect("ladder exhaustion still ships diagnostics");
        assert_eq!(partial.blocks.len(), 1);
        assert_eq!(partial.residuals.len(), 1);
        // The residual is evaluated at the restored initial guess (0.5 - 5).
        assert!(
            (partial.residuals[0].residual + 4.5).abs() < 1e-12,
            "{:?}",
            partial.residuals
        );
    }
}
