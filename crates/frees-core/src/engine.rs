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
//! # Built-in constants are knowns, not unknowns
//!
//! `AstBuilder` in the Java engine folds `pi#`/`R#`/`g#` into numeric literals
//! at parse time. This port's expression parser deliberately does not (there is
//! no `ConstantsRegistry` module for it to call), so a `#`-suffixed constant
//! reaches the solver as an ordinary [`crate::ast::Expr::Var`]. Left alone it
//! would count as a free variable and turn `a = pi#` into an underdetermined
//! system. The engine therefore resolves every variable that
//! [`crate::eval::lookup_constant`] knows, seeds its SI value into the scope and
//! hands it to the blocker as a *known*. The observable behaviour matches Java.
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
use crate::parser::{parse_document, Document, GuessDirective};
use crate::solver::blocker::{block_system, unknowns, Block};
use crate::solver::newton::{newton_solve, SolverSettings};
use crate::units::registry::UnitRegistry;

/// Initial value for an unknown with no `GUESS`.
///
/// `EquationSystemSolver.DEFAULT_GUESS` in the Java engine.
pub const DEFAULT_GUESS: f64 = 1.0;

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
    /// The blocks, in the order they were solved.
    pub blocks: Vec<Block>,
    /// Parser diagnostics plus anything the solve wanted to say. Unit and
    /// bounds problems are warnings and never block a solve.
    pub diagnostics: Vec<Diagnostic>,
    /// Total Newton iterations across every block.
    pub iterations: usize,
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
    /// Human-readable summary — the "No syntax errors were detected…" sentence
    /// on success, or the blocker's causality diagnosis on failure.
    pub message: String,
    pub diagnostics: Vec<Diagnostic>,
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

/// Parse, block and solve `source`.
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
pub fn solve(source: &str, settings: &SolverSettings) -> Result<Solution> {
    let doc = parse_document(source)?;
    reject_unsupported(&doc.statements)?;

    let equations: Vec<Equation> = doc.equations().into_iter().cloned().collect();
    let (constants, knowns) = builtin_constants(&equations);
    let report = block_system(&equations, &knowns)?;

    let mut diagnostics = doc.diagnostics.clone();
    collect_unit_warnings(&equations, &mut diagnostics);
    let specs = variable_specs(&equations, &knowns, &doc, &mut diagnostics);

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
        iterations += solve_block(index, block, &equations, &mut values, settings)?;
    }

    check_bounds(&specs, &values, &mut diagnostics);

    // Report the *unknowns*, not the whole scope. The built-in constants were
    // seeded into the scope so the evaluator could read them (see the module
    // docs), but the Java engine substitutes them as literals at parse time and
    // never surfaces them as result variables — `fixtures/golden/constants.json`
    // lists only `a`, `b`, `c`. Leaking `pi#` into the result table would be a
    // visible parity divergence.
    let solved = specs
        .keys()
        .map(|name| {
            let value = values.get(name).copied().unwrap_or(f64::NAN);
            (name.clone(), value)
        })
        .collect();

    Ok(Solution {
        values: solved,
        blocks: report.blocks,
        diagnostics,
        iterations,
    })
}

/// Verify syntax and structural solvability without solving.
///
/// Mirrors `POST /api/check`. A *syntax* failure is an `Err` (the Java endpoint
/// answers 400 for it); a *structural* failure is an `Ok` report with
/// `solvable: false` and the blocker's diagnosis in `message`, because the
/// editor needs the counts and the variable list either way.
pub fn check(source: &str) -> Result<CheckReport> {
    let doc = parse_document(source)?;
    reject_unsupported(&doc.statements)?;

    let equations: Vec<Equation> = doc.equations().into_iter().cloned().collect();
    let (_, knowns) = builtin_constants(&equations);
    let variables = unknowns(&equations, &knowns);

    let mut diagnostics = doc.diagnostics.clone();
    collect_unit_warnings(&equations, &mut diagnostics);

    let base = CheckReport {
        solvable: false,
        equation_count: equations.len(),
        unknown_count: variables.len(),
        variables,
        message: String::new(),
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

/// Initial guesses and bounds for every unknown.
///
/// Port of `EquationSystemSolver.withTextGuesses` over the default specs: the
/// in-text `GUESS` wins over the default, the bounds come from the directive,
/// and the guess is clamped into them. A directive naming something that is not
/// an unknown is a warning, not an error — the Java engine merges it into a
/// spec map that the solver then never reads.
fn variable_specs(
    equations: &[Equation],
    knowns: &HashSet<String>,
    doc: &Document,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeMap<String, VarSpec> {
    let mut specs: BTreeMap<String, VarSpec> = unknowns(equations, knowns)
        .into_iter()
        .map(|name| (name, VarSpec::default()))
        .collect();

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

    specs
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
        assert!(matches!(err, FreesError::Solver { .. }), "{err:?}");
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
        assert!(matches!(err, FreesError::Parse { .. }), "{err:?}");
        assert!(err.to_string_message().contains("COMPONENT"));
    }

    #[test]
    fn a_syntax_error_is_a_parse_error_with_a_span() {
        let err = solve("x = = 2\n", &SolverSettings::default()).unwrap_err();
        assert!(matches!(err, FreesError::Parse { .. }), "{err:?}");
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
    fn check_errors_on_a_syntax_error() {
        assert!(matches!(
            check("x = = 2").unwrap_err(),
            FreesError::Parse { .. }
        ));
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
    fn document_variables_includes_constants() {
        let doc = parse_document("a = pi# + b\n").unwrap();
        let vars: Vec<String> = document_variables(&doc).into_iter().collect();
        assert_eq!(vars, vec!["a", "b", "pi#"]);
        assert_eq!(
            known_constants(&doc.equations().into_iter().cloned().collect::<Vec<_>>())
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["pi#".to_string()]
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
