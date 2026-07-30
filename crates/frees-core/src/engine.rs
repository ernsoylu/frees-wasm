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
//! 1. **Parse.** [`crate::parser::parse_document`]. A block construct the wasm
//!    port has not reached yet is an explicit error, never a silent skip.
//! 2. **Collect equations.** [`crate::parser::Document::equations`] flattens
//!    `FOR` bodies. Statements that need machinery this port does not have
//!    (`CALL` into a `PROCEDURE`/`MODULE`, `SYMBOLIC`) are refused by name
//!    rather than dropped — dropping them would silently change the degrees of
//!    freedom and produce a confidently wrong answer.
//! 3. **Seed.** Every unknown starts at [`DEFAULT_GUESS`] (`1.0`, the Java
//!    `EquationSystemSolver.DEFAULT_GUESS`) with bounds `±∞`; in-text `GUESS`
//!    directives override the guess and narrow the bounds, and the guess is
//!    clamped into the bounds exactly as `withTextGuesses` does.
//! 4. **Block.** [`crate::solver::blocker::block_system`] — degrees of freedom,
//!    maximum bipartite matching, Tarjan SCC. The blocks come out in solve
//!    order.
//! 5. **Solve.** [`crate::solver::newton::newton_solve`] per block, in that
//!    order. Values solved by an earlier block are already in the shared scope
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
//! variables behind) but keeps hand-built ASTs and the future component
//! expander honest.
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
use crate::eval::{eval, lookup_constant, Scope};
use crate::lexer::tokenize;
use crate::parser::{parse_document, Document, GuessDirective};
use crate::solver::blocker::{block_system, unknowns, Block};
use crate::solver::newton::{newton_solve, SolverSettings};
use crate::token::TokenKind;
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
    /// Lowercase canonical name → first-seen source spelling, covering every
    /// unknown in the system.
    pub display_names: BTreeMap<String, String>,
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
    let doc = parse_document(source)?;
    reject_unsupported(&doc.statements)?;

    let equations: Vec<Equation> = doc.equations().into_iter().cloned().collect();
    let (constants, knowns) = builtin_constants(&equations);
    let report = block_system(&equations, &knowns)?;

    let mut diagnostics = doc.diagnostics.clone();
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

    let mut iterations = 0usize;
    for (index, block) in report.blocks.iter().enumerate() {
        match solve_block(index, block, &equations, &mut values, settings) {
            Ok(block_iterations) => iterations += block_iterations,
            Err(error) => {
                // The Java `enrichWithPartialResult`: attach the block
                // structure, every equation's residual at the stalled iterate
                // (`residuals_at` records NaN where evaluation fails), and
                // partial stats, so a failure ships diagnostics.
                let (residuals, max_residual) = residuals_at(&equations, &report.blocks, &values);
                let block_equations = block_equation_texts(&report.blocks, &equations);
                let display_names = display_names_for(specs.keys(), source);
                return Err(SolveFailure {
                    error,
                    failed_block_index: Some(index),
                    partial: Some(Box::new(PartialDiagnostics {
                        blocks: report.blocks,
                        block_equations,
                        display_names,
                        residuals,
                        stats: SolveStats {
                            iterations,
                            max_residual,
                            elapsed_ms: None,
                        },
                    })),
                });
            }
        }
    }

    check_bounds(&specs, &values, &mut diagnostics);

    // Report the *unknowns*, not the whole scope. The built-in constants were
    // seeded into the scope so the evaluator could read them (see the module
    // docs), but the Java engine substitutes them as literals at parse time and
    // never surfaces them as result variables — `fixtures/golden/constants.json`
    // lists only `a`, `b`, `c`. Leaking `pi#` into the result table would be a
    // visible parity divergence.
    let solved: BTreeMap<String, f64> = specs
        .keys()
        .map(|name| {
            let value = values.get(name).copied().unwrap_or(f64::NAN);
            (name.clone(), value)
        })
        .collect();

    let display_names = display_names_for(solved.keys(), source);
    let (residuals, max_residual) = residuals_at(&equations, &report.blocks, &values);
    let block_equations = block_equation_texts(&report.blocks, &equations);

    // Dimensional check + SI unit inference (the Java solve path's
    // `checkUnits` + `unitsByLowerName`): declared units feed the checker, and
    // the result map is the derived units overlaid by the declared ones —
    // a declared unit always wins over a dimensionally derived one.
    let declared = declared_units(&equations, overrides);
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
    for o in overrides {
        override_spec(o)?;
    }

    let doc = match parse_document(source).and_then(|doc| {
        reject_unsupported(&doc.statements)?;
        Ok(doc)
    }) {
        Ok(doc) => doc,
        Err(err @ FreesError::Parse { .. }) => return Ok(syntax_failure_report(source, &err)),
        Err(other) => return Err(other),
    };

    let equations: Vec<Equation> = doc.equations().into_iter().cloned().collect();
    let (_, knowns) = builtin_constants(&equations);
    let variables = unknowns(&equations, &knowns);

    let mut diagnostics = doc.diagnostics.clone();
    collect_unit_warnings(&equations, &mut diagnostics);

    // Dimensional check + SI unit inference (the Java `CheckController` path):
    // declared units — annotated literals plus external `VariableInfo` units —
    // feed the checker; the reported map is the derived units overlaid by the
    // *literal*-declared ones only. External units are deliberately left out of
    // the report (the caller already knows them), exactly as the Java check
    // response composes `deriveUnits` + `inferUnits`.
    let declared = declared_units(&equations, overrides);
    let unit_report = crate::units::checker::check_units(&equations, &declared);
    let mut inferred_units = unit_report.inferred;
    inferred_units.extend(literal_units(&equations));

    let base = CheckReport {
        solvable: false,
        equation_count: equations.len(),
        unknown_count: variables.len(),
        display_names: display_names_for(variables.iter(), source),
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

/// Refuse a statement that parses but that the engine cannot honour.
///
/// `Document::equations` walks past `SYMBOLIC` and `CALL` without comment. That
/// is fine for a structural walk and fatal for a solve: dropping a `CALL` drops
/// the equations that bind its outputs, so the system silently becomes
/// underdetermined — or worse, accidentally square. Naming the construct is the
/// only honest option until the procedure flattener lands.
fn reject_unsupported(statements: &[Statement]) -> Result<()> {
    for statement in statements {
        match statement {
            Statement::Eq(_) => {}
            Statement::For { body, .. } => reject_unsupported(body)?,
            Statement::Symbolic(names) => {
                return Err(FreesError::parse(format!(
                    "`SYMBOLIC` is not supported by the wasm engine yet (declared: {})",
                    names.join(", ")
                )))
            }
            Statement::CallProc { name, .. } => {
                return Err(FreesError::parse(format!(
                    "`CALL {name}` is not supported by the wasm engine yet: \
                     PROCEDURE/MODULE flattening is not ported"
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

/// Lowercase name → the spelling of its first appearance in the token stream.
///
/// The Java parser records this as it builds variables
/// (`ParseResult.displayNames`); this port reconstructs it with a plain lexer
/// pass — [`TokenKind::Ident`] keeps the original case, and the **first**
/// occurrence of each identifier wins, sigil suffixes (`$`, `#`) kept as-is.
/// The scan sees *every* identifier (function names, unit spellings inside
/// `[...]`), which is why callers filter the map down to actual variables via
/// [`display_names_for`] — that filtered view is what the golden fixtures
/// record.
fn first_seen_spellings(source: &str) -> HashMap<String, String> {
    let mut spellings = HashMap::new();
    if let Ok(tokens) = tokenize(source) {
        for token in tokens {
            if let TokenKind::Ident(text) = token.kind {
                spellings.entry(text.to_ascii_lowercase()).or_insert(text);
            }
        }
    }
    spellings
}

/// The display-name map for exactly `names` (lowercase canonical names): each
/// maps to its first-seen source spelling, or to itself when the scan has no
/// entry (the Java `displayNames.getOrDefault(v, v)`).
fn display_names_for<'a>(
    names: impl Iterator<Item = &'a String>,
    source: &str,
) -> BTreeMap<String, String> {
    let spellings = first_seen_spellings(source);
    names
        .map(|name| {
            let display = spellings.get(name).cloned().unwrap_or_else(|| name.clone());
            (name.clone(), display)
        })
        .collect()
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
) -> BTreeMap<String, String> {
    let mut units = literal_units(equations);
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

    let mut residuals = Vec::with_capacity(equations.len());
    let mut max_residual = 0.0f64;
    for (index, equation) in equations.iter().enumerate() {
        let residual = match (eval(&equation.lhs, values), eval(&equation.rhs, values)) {
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

/// Solve one block in place, leaving its unknowns' values in `values`.
///
/// Returns the Newton iteration count.
fn solve_block(
    index: usize,
    block: &Block,
    equations: &[Equation],
    values: &mut Scope,
    settings: &SolverSettings,
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
    residuals_into(&block_equations, values, &mut probe)
        .map_err(|err| annotate(err, index, &block_equations))?;

    let mut x: Vec<f64> = block
        .variables
        .iter()
        .map(|name| values.get(name).copied().unwrap_or(DEFAULT_GUESS))
        .collect();

    let names = &block.variables;
    let outcome = {
        let scope = &mut *values;
        newton_solve(
            |x: &[f64], out: &mut [f64]| {
                for (name, value) in names.iter().zip(x) {
                    scope.insert(name.clone(), *value);
                }
                match residuals_into(&block_equations, scope, out) {
                    Ok(()) => Ok(()),
                    // An invalid region, not a broken document: hand Newton the
                    // non-finite residual its line search knows how to reject.
                    Err(FreesError::Evaluation { .. }) | Err(FreesError::Property { .. }) => {
                        out.fill(f64::NAN);
                        Ok(())
                    }
                    Err(other) => Err(other),
                }
            },
            &mut x,
            settings,
        )
    };

    // Write back before propagating: on failure the last iterate is what makes
    // a stall report actionable, and it is what the Java engine leaves behind.
    for (name, value) in names.iter().zip(&x) {
        values.insert(name.clone(), *value);
    }

    let report = outcome.map_err(|err| annotate(err, index, &block_equations))?;
    Ok(report.iterations)
}

/// `out[k] = lhs_k(x) - rhs_k(x)` for each equation in the block.
fn residuals_into(equations: &[&Equation], scope: &Scope, out: &mut [f64]) -> Result<()> {
    for (slot, equation) in out.iter_mut().zip(equations) {
        *slot = eval(&equation.lhs, scope)? - eval(&equation.rhs, scope)?;
    }
    Ok(())
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
/// [`newton_solve`] has no bounds parameter (see its *Deviations*), so bounds
/// are advisory here: they seed the starting point and are reported afterwards.
/// Reporting is deliberate — silently returning an out-of-range answer under a
/// document that asked for a range is the kind of quiet wrongness the parent
/// engine's "Constrained solution" diagnostic exists to prevent.
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
        let err = solve("COMPONENT pump\nEND\n", &SolverSettings::default()).unwrap_err();
        assert!(matches!(err.error, FreesError::Parse { .. }), "{err:?}");
        assert!(err.to_string_message().contains("COMPONENT"));
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
        let solution = solved("FOR i = 1 TO 2\n  a = 5\nEND\nb = a + 1\n");
        assert_close(value(&solution, "a"), 5.0);
        assert_close(value(&solution, "b"), 6.0);
    }

    #[test]
    fn symbolic_statements_are_refused_rather_than_dropped() {
        let err = solve("SYMBOLIC s\nx = 1\n", &SolverSettings::default()).unwrap_err();
        assert!(err.to_string_message().contains("SYMBOLIC"), "{err}");
    }

    #[test]
    fn call_statements_are_refused_rather_than_dropped() {
        let err = solve("CALL mix(1, 2 : y)\nx = 1\n", &SolverSettings::default()).unwrap_err();
        let message = err.to_string_message();
        assert!(message.contains("mix"), "{message}");
        assert!(message.contains("not supported"), "{message}");
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

    #[test]
    fn out_of_bounds_results_warn_but_still_solve() {
        // The equation forces x = 5 while the GUESS declares 0..1.
        let solution = solved("GUESS x = 0.5 [0, 1]\nx = 5\n");
        assert_close(value(&solution, "x"), 5.0);
        assert!(
            solution
                .diagnostics
                .iter()
                .any(|d| d.message.contains("outside the GUESS bounds")),
            "{:?}",
            solution.diagnostics
        );
    }
}
