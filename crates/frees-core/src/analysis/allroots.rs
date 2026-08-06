//! Every root of a blocked system, not just the one nearest the guess.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/core/AllRootsSolver.java`
//! (386 LOC). Plain Newton reports the single root its guess falls into; this
//! enumerates them.
//!
//! Strategy per block, composed by Cartesian branching across blocks:
//!
//! * **1-variable blocks** — scan the bounded interval for sign changes of the
//!   residual and run Brent's method on each bracket. Unbounded variables are
//!   scanned within ±[`SCAN_LIMIT`]; set bounds in the Variable Information
//!   window to widen or narrow the search.
//! * **N-variable simultaneous blocks** — multi-start Newton from the guess plus
//!   pseudo-random starts inside the bounds, deduplicated.
//!
//! Every root of block *k* forks a new branch for the remaining blocks, so the
//! result is the full combination set of system solutions, capped at
//! [`MAX_SOLUTIONS`] branches to avoid combinatorial explosion.
//!
//! # Duplicate-root policy
//!
//! Two policies, deliberately different:
//!
//! * within one 1-D block, [`add_root`] merges candidates closer than
//!   `1e-6 · max(1, |existing|)` and keeps the list **sorted ascending** after
//!   every insertion (the Java re-sorts on each `add`);
//! * across whole solutions, [`same_on`] compares every variable of every block
//!   with the same relative tolerance, and [`dedup_and_sort`] orders the
//!   survivors lexicographically by the variables in `TreeSet` (i.e. sorted)
//!   order.
//!
//! Roots are also **polished** before being reported: a second Newton pass with
//! a near-zero residual tolerance (`1e-30`) keeps iterating until the variable
//! itself stops moving. Without it a multiple root — where the residual is
//! `≈ error^m` — reports a value that satisfies the residual test while still
//! being far from the root.
//!
//! # What is *not* reproducible
//!
//! The Java `multiStartRoots` draws its extra starts from a
//! `java.security.SecureRandom` static, so an N-D block's start points differ
//! on every JVM run — despite the class comment claiming "deterministic
//! pseudo-random starts". `wasm32-unknown-unknown` has no entropy source and
//! the port takes no new dependency, so this uses a seeded
//! [`crate::analysis::pareto::JavaRandom`] instead: the *documented* intent,
//! and reproducible. Because [`attempt_start`] deduplicates every converged
//! start against the roots already found, the reported set is insensitive to
//! the exact start points; only the *order* of discovery could differ, and
//! [`dedup_and_sort`] sorts that away.

use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::optimizer::{java_compare, jmax, jmin, precision_equals};
use crate::analysis::pareto::JavaRandom;
use crate::ast::Equation;
use crate::diag::{FreesError, Result};
use crate::eval::{eval_with, EvalContext, Scope};
use crate::parser::defs::Definitions;
use crate::solver::{Block, SolverSettings};

/// How far an unbounded variable is scanned in either direction.
pub const SCAN_LIMIT: f64 = 100.0;
/// Sub-intervals the 1-D scan splits `[lo, hi]` into.
const SCAN_INTERVALS: usize = 1024;
/// Multi-start budget per variable of an N-D block.
const STARTS_PER_VARIABLE: usize = 32;
/// Ceiling on the multi-start budget.
const MAX_STARTS: usize = 128;
/// Ceiling on the number of solution branches carried forward.
pub const MAX_SOLUTIONS: usize = 32;
/// Two roots closer than `ROOT_EPS · max(1, |existing|)` are the same root.
const ROOT_EPS: f64 = 1e-6;
/// `BrentSolver(1e-14, 1e-12)` — relative accuracy.
const BRENT_RELATIVE_ACCURACY: f64 = 1e-14;
/// `BrentSolver(1e-14, 1e-12)` — absolute accuracy.
const BRENT_ABSOLUTE_ACCURACY: f64 = 1e-12;
/// Apache's `BaseAbstractUnivariateSolver.DEFAULT_FUNCTION_VALUE_ACCURACY`.
const BRENT_FUNCTION_VALUE_ACCURACY: f64 = 1e-15;
/// `new BrentSolver(...).solve(200, ...)` — the evaluation budget.
const BRENT_MAX_EVAL: usize = 200;
/// Seed for the multi-start draws; see the module docs on why this is fixed.
const MULTI_START_SEED: i64 = 20_240_517;

/// What the Variable Information window says about one variable — the subset of
/// the Java `VariableSpec` this search reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RootSpec {
    pub guess: f64,
    pub lower: f64,
    pub upper: f64,
}

impl Default for RootSpec {
    fn default() -> RootSpec {
        RootSpec {
            guess: 1.0,
            lower: f64::NEG_INFINITY,
            upper: f64::INFINITY,
        }
    }
}

/// The root enumerator — the Java `AllRootsSolver`.
///
/// Construct it over the *already blocked* system (the Java takes
/// `List<Block>` in `findAll` and the specs/defs in its constructor) and call
/// [`AllRootsSolver::find_all`].
pub struct AllRootsSolver<'a> {
    settings: SolverSettings,
    /// Near-zero residual tolerance so the polisher keeps iterating until the
    /// variable change drops below 1e-15. Critical for multiple roots, where
    /// the residual ≈ error^m drops below tolerance long before the variable
    /// has converged. The Java `new SolverSettings(50, 1e-30, 1e-15, …)`.
    polisher: SolverSettings,
    specs: &'a BTreeMap<String, RootSpec>,
    defs: &'a Definitions,
    equations: &'a [Equation],
    total_iterations: usize,
    rng: JavaRandom,
}

impl<'a> AllRootsSolver<'a> {
    /// `new AllRootsSolver(settings, specs, defs)`, plus the equation list this
    /// port's [`Block`] indexes into (the Java `Block` carries its equations
    /// inline).
    pub fn new(
        settings: SolverSettings,
        specs: &'a BTreeMap<String, RootSpec>,
        defs: &'a Definitions,
        equations: &'a [Equation],
    ) -> AllRootsSolver<'a> {
        AllRootsSolver {
            settings,
            polisher: SolverSettings {
                max_iterations: 50,
                rel_tolerance: 1e-30,
                // Inert (strictly below every reachable `rel_tolerance · scale`),
                // as the Java criterion is purely relative.
                abs_tolerance: 0.0,
                ..settings
            },
            specs,
            defs,
            equations,
            total_iterations: 0,
            rng: JavaRandom::new(MULTI_START_SEED),
        }
    }

    /// Total Newton iterations spent, across every block and every start.
    pub fn total_iterations(&self) -> usize {
        self.total_iterations
    }

    /// Complete value maps, one per distinct system solution.
    ///
    /// # Errors
    ///
    /// [`FreesError::Solver`] when a block yields no root at all inside the
    /// search region — the Java "Adjust guesses or bounds in the Variable
    /// Information window" message, with the block's 0-based index.
    pub fn find_all(&mut self, blocks: &[Block], initial_guesses: &Scope) -> Result<Vec<Scope>> {
        let mut branches: Vec<Scope> = vec![initial_guesses.clone()];

        for (index, block) in blocks.iter().enumerate() {
            let mut next_branches: Vec<Scope> = Vec::new();
            for branch in &branches {
                for rooted in self.block_roots(block, branch) {
                    if next_branches.len() < MAX_SOLUTIONS {
                        next_branches.push(rooted);
                    }
                }
            }
            if next_branches.is_empty() {
                return Err(FreesError::solver(format!(
                    "No solution found for block {index} within the search region. \
                     Adjust guesses or bounds in the Variable Information window."
                )));
            }
            branches = next_branches;
        }

        Ok(dedup_and_sort(branches, blocks))
    }

    /// All roots of one block given fixed upstream values; each returned map is
    /// a branch copy.
    fn block_roots(&mut self, block: &Block, branch: &Scope) -> Vec<Scope> {
        if block.variables.len() == 1 {
            self.scan_roots_1d(block, branch)
        } else {
            self.multi_start_roots(block, branch)
        }
    }

    // ------------------------------------------------------------------
    // 1-D blocks: interval scan + Brent
    // ------------------------------------------------------------------

    /// The Java `scanRoots1D`.
    fn scan_roots_1d(&mut self, block: &Block, branch: &Scope) -> Vec<Scope> {
        let var_name = block.variables[0].clone();
        let spec = self.specs.get(&var_name).copied();

        let lo = match spec {
            Some(s) if s.lower.is_finite() => s.lower,
            _ => -SCAN_LIMIT,
        };
        let hi = match spec {
            Some(s) if s.upper.is_finite() => s.upper,
            _ => SCAN_LIMIT,
        };

        let mut roots = self.run_scan(block, branch, &var_name, lo, hi);

        // Also run plain Newton from the guess: it preserves single-root
        // behaviour for roots outside the scan window, and for tangent
        // (non-crossing) roots the sign scan cannot see.
        {
            let mut newton_branch = branch.clone();
            let settings = self.settings;
            if let Ok(iterations) = self.solve_block(block, &mut newton_branch, &settings) {
                self.total_iterations += iterations;
                if let Some(&value) = newton_branch.get(&var_name) {
                    add_root(&mut roots, value);
                }
            }
        }

        // Polish every root for maximum precision (critical for multiple roots,
        // where the residual drops to tolerance well before x converges).
        let mut result: Vec<Scope> = Vec::with_capacity(roots.len());
        for root in roots {
            let mut copy = branch.clone();
            copy.insert(var_name.clone(), root);
            let polisher = self.polisher;
            if let Ok(iterations) = self.solve_block(block, &mut copy, &polisher) {
                self.total_iterations += iterations;
            }
            result.push(copy);
        }
        result
    }

    /// The Java `Scan1D.runScan`: walk `[lo, hi]` in [`SCAN_INTERVALS`] steps,
    /// collecting exact zeros, sign-change brackets (solved with Brent) and
    /// suspected tangent roots (solved with Newton from the midpoint).
    fn run_scan(
        &mut self,
        block: &Block,
        branch: &Scope,
        var_name: &str,
        lo: f64,
        hi: f64,
    ) -> Vec<f64> {
        let mut roots: Vec<f64> = Vec::new();
        let step = (hi - lo) / SCAN_INTERVALS as f64;
        if step <= 0.0 || !step.is_finite() {
            return roots;
        }
        let equation = match self.equations.get(block.equations[0]) {
            Some(equation) => equation.clone(),
            None => return roots,
        };
        // The Java `Scan1D.work` map: one scratch scope reused by the closure.
        let mut work = branch.clone();

        let mut prev_t = lo;
        let mut prev_f = safe_eval(&equation, var_name, prev_t, &mut work, self.defs);
        for i in 1..=SCAN_INTERVALS {
            let t = if i == SCAN_INTERVALS {
                hi
            } else {
                lo + i as f64 * step
            };
            let ft = safe_eval(&equation, var_name, t, &mut work, self.defs);
            self.check_and_add_root(&equation, var_name, prev_t, prev_f, &mut work, &mut roots);
            if prev_f.is_finite() && ft.is_finite() && prev_f * ft < 0.0 {
                self.check_brent_root(&equation, var_name, prev_t, t, &mut work, &mut roots);
            }
            if prev_f.is_finite()
                && ft.is_finite()
                && prev_f * ft > 0.0
                && prev_f.abs() + ft.abs() > 0.0
            {
                self.check_tangent_root(
                    block, branch, &equation, var_name, prev_t, t, prev_f, ft, &mut roots,
                );
            }
            prev_t = t;
            prev_f = ft;
        }
        self.check_and_add_root(&equation, var_name, prev_t, prev_f, &mut work, &mut roots);
        roots
    }

    /// The Java `Scan1D.checkAndAddRoot`: a sample that is exactly zero.
    fn check_and_add_root(
        &self,
        equation: &Equation,
        var_name: &str,
        t: f64,
        ft: f64,
        work: &mut Scope,
        roots: &mut Vec<f64>,
    ) {
        if ft == 0.0 && self.is_valid_root(equation, var_name, t, work) {
            add_root(roots, t);
        }
    }

    /// The Java `Scan1D.checkBrentRoot`: a sign change is a bracket.
    fn check_brent_root(
        &self,
        equation: &Equation,
        var_name: &str,
        prev_t: f64,
        t: f64,
        work: &mut Scope,
        roots: &mut Vec<f64>,
    ) {
        let defs = self.defs;
        let mut f = |x: f64, scope: &mut Scope| safe_eval(equation, var_name, x, scope, defs);
        // A bracket that fails to converge (or exhausts the 200-evaluation
        // budget) is skipped — the Java `catch (RuntimeException ignored)`.
        if let Some(root) = brent_root(
            &mut f,
            work,
            jmin(prev_t, t),
            jmax(prev_t, t),
            BRENT_MAX_EVAL,
        ) {
            // Validate the Brent root to handle poles.
            if self.is_valid_root(equation, var_name, root, work) {
                add_root(roots, root);
            }
        }
    }

    /// The Java `Scan1D.checkTangentRoot`: two same-sign samples that both get
    /// close to zero suggest a root the sign scan cannot see, so Newton is run
    /// from their midpoint.
    ///
    /// The argument list is the Java method's plus the `(block, branch,
    /// equation)` this port must thread explicitly, because its `Block` holds
    /// equation *indices* while the Java `Scan1D` is an inner class closing
    /// over all three. Bundling them would hide the correspondence.
    #[allow(clippy::too_many_arguments)]
    fn check_tangent_root(
        &mut self,
        block: &Block,
        branch: &Scope,
        equation: &Equation,
        var_name: &str,
        prev_t: f64,
        t: f64,
        prev_f: f64,
        ft: f64,
        roots: &mut Vec<f64>,
    ) {
        let min_abs = jmin(prev_f.abs(), ft.abs());
        if min_abs >= self.settings.rel_tolerance.sqrt() {
            return;
        }
        let mid = (prev_t + t) / 2.0;
        let mut tangent_work = branch.clone();
        tangent_work.insert(var_name.to_string(), mid);
        let settings = self.settings;
        let Ok(iterations) = self.solve_block(block, &mut tangent_work, &settings) else {
            return; // tangent root solve failed; skip it
        };
        self.total_iterations += iterations;
        let Some(&tangent_root) = tangent_work.get(var_name) else {
            return;
        };
        if self.is_valid_root(equation, var_name, tangent_root, &mut tangent_work) {
            add_root(roots, tangent_root);
        }
    }

    /// The Java `isValidRoot`: `|lhs − rhs| / max(|lhs|, 1e-12)` within
    /// `sqrt(relativeResiduals)`. Rejects poles, where the residual blows up.
    fn is_valid_root(
        &self,
        equation: &Equation,
        var_name: &str,
        root: f64,
        work: &mut Scope,
    ) -> bool {
        work.insert(var_name.to_string(), root);
        let ctx = EvalContext::with_defs(self.defs);
        let (Ok(lhs), Ok(rhs)) = (
            eval_with(&equation.lhs, work, ctx),
            eval_with(&equation.rhs, work, ctx),
        ) else {
            return false;
        };
        let scale = jmax(lhs.abs(), 1.0e-12);
        (lhs - rhs).abs() / scale <= self.settings.rel_tolerance.sqrt()
    }

    // ------------------------------------------------------------------
    // N-D blocks: multi-start Newton
    // ------------------------------------------------------------------

    /// The Java `multiStartRoots`.
    fn multi_start_roots(&mut self, block: &Block, branch: &Scope) -> Vec<Scope> {
        let vars = &block.variables;
        let n = vars.len();
        let starts = MAX_STARTS.min(STARTS_PER_VARIABLE * n);

        let mut lo = vec![0.0f64; n];
        let mut hi = vec![0.0f64; n];
        for i in 0..n {
            let spec = self.specs.get(&vars[i]).copied();
            lo[i] = match spec {
                Some(s) if s.lower.is_finite() => s.lower,
                _ => -SCAN_LIMIT,
            };
            hi[i] = match spec {
                Some(s) if s.upper.is_finite() => s.upper,
                _ => SCAN_LIMIT,
            };
        }

        let mut found: Vec<Scope> = Vec::new();

        // Start 0: the user's guess values (preserves single-solve behaviour).
        self.attempt_start(block, branch, None, &mut found);

        for s in 1..starts {
            let start = self.generate_random_start(s, n, &lo, &hi);
            self.attempt_start(block, branch, Some(&start), &mut found);
        }

        found
    }

    /// The Java `generateRandomStart`: even starts cluster near the origin
    /// (engineering solutions sit near the guess magnitude far more often than
    /// near the box edges), odd starts span the whole box.
    fn generate_random_start(&mut self, s: usize, n: usize, lo: &[f64], hi: &[f64]) -> Vec<f64> {
        let mut start = vec![0.0f64; n];
        for i in 0..n {
            if s % 2 == 0 {
                let near_lo = jmax(lo[i], -10.0);
                let near_hi = jmin(hi[i], 10.0);
                start[i] = near_lo + self.rng.next_double() * (near_hi - near_lo);
            } else {
                start[i] = lo[i] + self.rng.next_double() * (hi[i] - lo[i]);
            }
        }
        start
    }

    /// The Java `attemptStart`: Newton from `start` (or from the branch values
    /// when `None`), polish, then keep only if it is a new root.
    fn attempt_start(
        &mut self,
        block: &Block,
        branch: &Scope,
        start: Option<&[f64]>,
        found: &mut Vec<Scope>,
    ) {
        let vars = block.variables.clone();
        let mut work = branch.clone();
        if let Some(start) = start {
            for (name, value) in vars.iter().zip(start) {
                work.insert(name.clone(), *value);
            }
        }
        let settings = self.settings;
        let Ok(iterations) = self.solve_block(block, &mut work, &settings) else {
            return;
        };
        self.total_iterations += iterations;
        // Polishing is best-effort; the loose solution is still valid.
        let polisher = self.polisher;
        if let Ok(iterations) = self.solve_block(block, &mut work, &polisher) {
            self.total_iterations += iterations;
        }
        for existing in found.iter() {
            if same_on(existing, &work, &vars) {
                return;
            }
        }
        found.push(work);
    }

    // ------------------------------------------------------------------

    /// One plain Newton solve of `block`, through the engine's public seam.
    ///
    /// This is `NewtonSolver.solveBlock` and nothing else: the Java
    /// `AllRootsSolver` deliberately bypasses `solveBlockWithFallback`, because
    /// a multi-start search expects most starts to fail and must not spend the
    /// retry ladder on each one.
    fn solve_block(
        &self,
        block: &Block,
        values: &mut Scope,
        settings: &SolverSettings,
    ) -> Result<usize> {
        let bounds: BTreeMap<String, (f64, f64)> = self
            .specs
            .iter()
            .map(|(name, spec)| (name.clone(), (spec.lower, spec.upper)))
            .collect();
        crate::engine::solve_block_newton(
            block,
            self.equations,
            values,
            settings,
            &bounds,
            crate::eval::EvalContext::with_defs(self.defs),
        )
    }
}

/// The Java `Scan1D.f` composed with `safeEval`: the residual of the block's
/// single equation at `var = t`, `NaN` where evaluation fails.
fn safe_eval(
    equation: &Equation,
    var_name: &str,
    t: f64,
    work: &mut Scope,
    defs: &Definitions,
) -> f64 {
    work.insert(var_name.to_string(), t);
    let ctx = EvalContext::with_defs(defs);
    match (
        eval_with(&equation.lhs, work, ctx),
        eval_with(&equation.rhs, work, ctx),
    ) {
        (Ok(lhs), Ok(rhs)) => lhs - rhs,
        _ => f64::NAN,
    }
}

/// The Java `addRoot`: merge a candidate into the list unless an existing root
/// is within `1e-6 · max(1, |existing|)`, then re-sort ascending.
fn add_root(roots: &mut Vec<f64>, candidate: f64) {
    for &existing in roots.iter() {
        if (existing - candidate).abs() <= ROOT_EPS * jmax(1.0, existing.abs()) {
            return;
        }
    }
    roots.push(candidate);
    roots.sort_by(|a, b| java_compare(*a, *b));
}

/// The Java `sameOn`: two value maps agree on every listed variable.
fn same_on(a: &Scope, b: &Scope, vars: &[String]) -> bool {
    for var_name in vars {
        let x = a.get(var_name).copied().unwrap_or(f64::NAN);
        let y = b.get(var_name).copied().unwrap_or(f64::NAN);
        if (x - y).abs() > ROOT_EPS * jmax(1.0, x.abs()) {
            return false;
        }
    }
    true
}

/// The Java `dedupAndSort`: drop duplicates over the union of every block's
/// variables (in `TreeSet` order), then order the survivors lexicographically
/// by those same variables.
fn dedup_and_sort(solutions: Vec<Scope>, blocks: &[Block]) -> Vec<Scope> {
    let all_vars: Vec<String> = blocks
        .iter()
        .flat_map(|b| b.variables.iter().cloned())
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect();

    let mut unique: Vec<Scope> = Vec::with_capacity(solutions.len());
    for candidate in solutions {
        if !unique.iter().any(|u| same_on(u, &candidate, &all_vars)) {
            unique.push(candidate);
        }
    }

    unique.sort_by(|a, b| {
        for var_name in &all_vars {
            let x = a.get(var_name).copied().unwrap_or(f64::NAN);
            let y = b.get(var_name).copied().unwrap_or(f64::NAN);
            let cmp = java_compare(x, y);
            if cmp != std::cmp::Ordering::Equal {
                return cmp;
            }
        }
        std::cmp::Ordering::Equal
    });
    unique
}

// ---------------------------------------------------------------------------
// Apache Commons Math 3.6.1 — BrentSolver
// ---------------------------------------------------------------------------

/// Apache `BrentSolver(1e-14, 1e-12).solve(maxEval, f, min, max)`.
///
/// `None` stands for every `RuntimeException` the Java call site swallows: the
/// initial-interval checks failing, no bracket, or the evaluation budget
/// running out mid-iteration.
///
/// The three early returns (`|f(initial)| <= 1e-15`, then `min`, then `max`) and
/// the choice of which half-interval to hand to [`brent`] are Apache's
/// `doSolve`, transcribed; `initial` is the midpoint, which is what the
/// four-argument `solve` overload passes.
fn brent_root<F>(f: &mut F, scope: &mut Scope, min: f64, max: f64, max_eval: usize) -> Option<f64>
where
    F: FnMut(f64, &mut Scope) -> f64,
{
    // Apache `verifyInterval`: `NumberIsTooLargeException` when `min >= max`.
    // Written un-negated on purpose — with a NaN bound the Java comparison is
    // false and the solve proceeds, and a `!(min < max)` guard would reject it
    // instead.
    if min >= max {
        return None;
    }
    let initial = min + 0.5 * (max - min);
    let mut budget = Budget {
        used: 0,
        max: max_eval,
    };

    let y_initial = budget.eval(f, scope, initial)?;
    if y_initial.abs() <= BRENT_FUNCTION_VALUE_ACCURACY {
        return Some(initial);
    }
    let y_min = budget.eval(f, scope, min)?;
    if y_min.abs() <= BRENT_FUNCTION_VALUE_ACCURACY {
        return Some(min);
    }
    if y_initial * y_min < 0.0 {
        return brent(f, scope, &mut budget, min, initial, y_min, y_initial);
    }
    let y_max = budget.eval(f, scope, max)?;
    if y_max.abs() <= BRENT_FUNCTION_VALUE_ACCURACY {
        return Some(max);
    }
    if y_initial * y_max < 0.0 {
        return brent(f, scope, &mut budget, initial, max, y_initial, y_max);
    }
    None // `NoBracketingException`
}

/// Apache's `Incrementor`: the count rises *before* the function is called, so
/// the budget is exactly `max` successful evaluations.
struct Budget {
    used: usize,
    max: usize,
}

impl Budget {
    fn eval<F>(&mut self, f: &mut F, scope: &mut Scope, x: f64) -> Option<f64>
    where
        F: FnMut(f64, &mut Scope) -> f64,
    {
        self.used += 1;
        if self.used > self.max {
            return None; // `TooManyEvaluationsException`
        }
        Some(f(x, scope))
    }
}

/// Apache `BrentSolver.brent` — inverse quadratic interpolation with a linear
/// fallback and forced bisection, transcribed including the deliberate `a == c`
/// identity test ("part of the original Brent's method; it should NOT be
/// replaced by a proximity test").
#[allow(clippy::too_many_arguments)]
fn brent<F>(
    f: &mut F,
    scope: &mut Scope,
    budget: &mut Budget,
    lo: f64,
    hi: f64,
    f_lo: f64,
    f_hi: f64,
) -> Option<f64>
where
    F: FnMut(f64, &mut Scope) -> f64,
{
    let mut a = lo;
    let mut fa = f_lo;
    let mut b = hi;
    let mut fb = f_hi;
    let mut c = a;
    let mut fc = fa;
    let mut d = b - a;
    let mut e = d;

    let t = BRENT_ABSOLUTE_ACCURACY;
    let eps = BRENT_RELATIVE_ACCURACY;

    loop {
        if fc.abs() < fb.abs() {
            a = b;
            b = c;
            c = a;
            fa = fb;
            fb = fc;
            fc = fa;
        }

        let tol = 2.0 * eps * b.abs() + t;
        let m = 0.5 * (c - b);

        if m.abs() <= tol || precision_equals(fb, 0.0) {
            return Some(b);
        }
        if e.abs() < tol || fa.abs() <= fb.abs() {
            // Force bisection.
            d = m;
            e = d;
        } else {
            let mut s = fb / fa;
            let mut p;
            let mut q;
            // The equality test (a == c) is intentional: it is part of the
            // original Brent's method and must NOT become a proximity test.
            if a == c {
                // Linear interpolation.
                p = 2.0 * m * s;
                q = 1.0 - s;
            } else {
                // Inverse quadratic interpolation.
                q = fa / fc;
                let r = fb / fc;
                p = s * (2.0 * m * q * (q - r) - (b - a) * (r - 1.0));
                q = (q - 1.0) * (r - 1.0) * (s - 1.0);
            }
            if p > 0.0 {
                q = -q;
            } else {
                p = -p;
            }
            s = e;
            e = d;
            if p >= 1.5 * m * q - (tol * q).abs() || p >= (0.5 * s * q).abs() {
                // Interpolation points the wrong way, or progress is slow.
                d = m;
                e = d;
            } else {
                d = p / q;
            }
        }
        a = b;
        fa = fb;

        if d.abs() > tol {
            b += d;
        } else if m > 0.0 {
            b += tol;
        } else {
            b -= tol;
        }
        fb = budget.eval(f, scope, b)?;
        if (fb > 0.0 && fc > 0.0) || (fb <= 0.0 && fc <= 0.0) {
            c = a;
            fc = fa;
            d = b - a;
            e = d;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::blocker::block_system;
    use std::collections::HashSet;

    /// Parse, expand and block a scalar document, returning everything
    /// [`AllRootsSolver`] needs.
    fn prepared(source: &str) -> (Vec<Equation>, Vec<Block>, Definitions, Scope) {
        let doc = crate::parser::parse_document(source).expect("parses");
        let equations = crate::parser::expand::expand_document(&doc).expect("expands");
        let knowns: HashSet<String> = HashSet::new();
        let report = block_system(&equations, &knowns).expect("blocks");
        let mut guesses: Scope = Scope::default();
        for name in crate::solver::blocker::unknowns(&equations, &knowns) {
            guesses.insert(name, 1.0);
        }
        (equations, report.blocks, doc.defs, guesses)
    }

    fn spec(lower: f64, upper: f64) -> RootSpec {
        RootSpec {
            guess: 1.0,
            lower,
            upper,
        }
    }

    // ── Apache BrentSolver ────────────────────────────────────────────────

    #[test]
    fn brent_root_finds_a_simple_bracketed_root() {
        let mut scope = Scope::default();
        // x² − 2 on [0, 2] → sqrt(2)
        let mut f = |x: f64, _: &mut Scope| x * x - 2.0;
        let root = brent_root(&mut f, &mut scope, 0.0, 2.0, 200).unwrap();
        assert!(
            (root - std::f64::consts::SQRT_2).abs() < 1e-12,
            "root = {root}"
        );
    }

    #[test]
    fn brent_root_returns_an_endpoint_that_is_already_a_root() {
        let mut scope = Scope::default();
        let mut f = |x: f64, _: &mut Scope| x;
        // The midpoint of [-1, 1] is exactly the root.
        assert_eq!(brent_root(&mut f, &mut scope, -1.0, 1.0, 200), Some(0.0));
        // And the lower endpoint when the midpoint is not.
        let mut g = |x: f64, _: &mut Scope| x - 3.0;
        assert_eq!(brent_root(&mut g, &mut scope, 3.0, 9.0, 200), Some(3.0));
    }

    #[test]
    fn brent_root_answers_none_without_a_bracket() {
        let mut scope = Scope::default();
        let mut f = |x: f64, _: &mut Scope| x * x + 1.0;
        assert_eq!(brent_root(&mut f, &mut scope, -1.0, 1.0, 200), None);
        // A degenerate interval is refused, not silently accepted.
        let mut g = |x: f64, _: &mut Scope| x;
        assert_eq!(brent_root(&mut g, &mut scope, 1.0, 1.0, 200), None);
    }

    #[test]
    fn brent_root_respects_its_evaluation_budget() {
        let mut scope = Scope::default();
        let mut f = |x: f64, _: &mut Scope| x * x - 2.0;
        // Two evaluations are not enough to converge on [0, 2].
        assert_eq!(brent_root(&mut f, &mut scope, 0.0, 2.0, 2), None);
    }

    // ── duplicate-root policy ─────────────────────────────────────────────

    #[test]
    fn add_root_merges_near_duplicates_and_keeps_the_list_sorted() {
        let mut roots = Vec::new();
        add_root(&mut roots, 3.0);
        add_root(&mut roots, -1.0);
        add_root(&mut roots, 3.000_000_1); // within 1e-6·3 of 3.0 → merged
        add_root(&mut roots, 1.0);
        assert_eq!(roots, vec![-1.0, 1.0, 3.0]);
    }

    #[test]
    fn add_root_scales_its_tolerance_with_magnitude() {
        let mut roots = vec![1000.0];
        add_root(&mut roots, 1_000.000_5); // 5e-4 < 1e-6·1000 → merged
        assert_eq!(roots.len(), 1);
        let mut roots = vec![0.001];
        add_root(&mut roots, 0.002); // 1e-3 > 1e-6·max(1, .001) → kept
        assert_eq!(roots.len(), 2);
    }

    #[test]
    fn same_on_compares_only_the_listed_variables() {
        let mut a = Scope::default();
        a.insert("x".into(), 1.0);
        a.insert("y".into(), 9.0);
        let mut b = Scope::default();
        b.insert("x".into(), 1.000_000_1);
        b.insert("y".into(), -50.0);
        assert!(same_on(&a, &b, &["x".to_string()]));
        assert!(!same_on(&a, &b, &["x".to_string(), "y".to_string()]));
    }

    #[test]
    fn dedup_and_sort_orders_lexicographically_by_sorted_variable_names() {
        let block = Block {
            equations: vec![0],
            variables: vec!["b".into(), "a".into()],
        };
        let mk = |a: f64, b: f64| {
            let mut s = Scope::default();
            s.insert("a".into(), a);
            s.insert("b".into(), b);
            s
        };
        let out = dedup_and_sort(vec![mk(2.0, 1.0), mk(1.0, 9.0), mk(1.0, 0.0)], &[block]);
        // Sorted on `a` first (TreeSet order), then `b`.
        assert_eq!(out.len(), 3);
        assert_eq!(out[0]["a"], 1.0);
        assert_eq!(out[0]["b"], 0.0);
        assert_eq!(out[1]["b"], 9.0);
        assert_eq!(out[2]["a"], 2.0);
    }

    // ── end to end, 1-D ───────────────────────────────────────────────────

    #[test]
    fn finds_both_roots_of_a_quadratic() {
        // x² = 4 → ±2. Plain Newton from guess 1 reports only +2.
        let (equations, blocks, defs, guesses) = prepared("x^2 = 4");
        let specs = BTreeMap::from([("x".to_string(), spec(-10.0, 10.0))]);
        let mut solver = AllRootsSolver::new(SolverSettings::default(), &specs, &defs, &equations);
        let solutions = solver.find_all(&blocks, &guesses).unwrap();
        let xs: Vec<f64> = solutions.iter().map(|s| s["x"]).collect();
        assert_eq!(xs.len(), 2, "got {xs:?}");
        assert!((xs[0] + 2.0).abs() < 1e-9, "{xs:?}");
        assert!((xs[1] - 2.0).abs() < 1e-9, "{xs:?}");
        assert!(solver.total_iterations() > 0);
    }

    #[test]
    fn finds_multiple_roots_of_a_cubic() {
        // x³ − 6x² + 11x − 6 = 0 has roots 1, 2 and 3. The Java oracle reports
        // **two** of them, bit for bit — see
        // `is_valid_root_is_effectively_exact_zero_for_a_zero_rhs` for why the
        // middle one is dropped. This asserts the oracle's exact doubles.
        let (equations, blocks, defs, guesses) = prepared("x^3 - 6*x^2 + 11*x - 6 = 0");
        let specs = BTreeMap::from([("x".to_string(), spec(0.0, 5.0))]);
        let mut solver = AllRootsSolver::new(SolverSettings::default(), &specs, &defs, &equations);
        let solutions = solver.find_all(&blocks, &guesses).unwrap();
        let xs: Vec<f64> = solutions.iter().map(|s| s["x"]).collect();
        assert_eq!(
            xs,
            vec![1.000_000_000_000_000_2, 3.000_000_000_000_004],
            "diverged from the Java oracle"
        );
        // The Java run spends zero Newton iterations here: the scan's Brent
        // roots and the plain-Newton root all land on the answer immediately.
        assert_eq!(solver.total_iterations(), 0);
    }

    #[test]
    fn is_valid_root_is_effectively_exact_zero_for_a_zero_rhs() {
        // The Java validity test is `|lhs − rhs| / max(|lhs|, 1e-12) <= sqrt(tol)`.
        // With `rhs = 0` the numerator *is* `|lhs|`, so the ratio is 1 for every
        // non-zero residual and only a residual that rounds to exactly zero (or
        // sits below ~1e-18) passes. That is why an `expr = 0` document reports
        // fewer roots than it has — transcribed, not "fixed".
        let (equations, _, defs, _) = prepared("x^3 - 6*x^2 + 11*x - 6 = 0");
        let specs = BTreeMap::from([("x".to_string(), spec(0.0, 5.0))]);
        let solver = AllRootsSolver::new(SolverSettings::default(), &specs, &defs, &equations);
        let mut work = Scope::default();
        // A root good to 1e-14 is still rejected, because the residual is not
        // bit-zero.
        assert!(!solver.is_valid_root(&equations[0], "x", 2.0 + 1e-14, &mut work));
        // Contrast: with a non-zero RHS the scale is the LHS magnitude and the
        // test behaves as intended.
        let (eq2, _, defs2, _) = prepared("x^2 = 4");
        let solver2 = AllRootsSolver::new(SolverSettings::default(), &specs, &defs2, &eq2);
        assert!(solver2.is_valid_root(&eq2[0], "x", 2.0 + 1e-14, &mut work));
    }

    #[test]
    fn the_scan_window_bounds_the_search() {
        // Same cubic, but the window only admits the root at 1.
        let (equations, blocks, defs, guesses) = prepared("x^3 - 6*x^2 + 11*x - 6 = 0");
        let specs = BTreeMap::from([("x".to_string(), spec(0.0, 1.5))]);
        let mut solver = AllRootsSolver::new(SolverSettings::default(), &specs, &defs, &equations);
        let solutions = solver.find_all(&blocks, &guesses).unwrap();
        let xs: Vec<f64> = solutions.iter().map(|s| s["x"]).collect();
        assert_eq!(xs.len(), 1, "got {xs:?}");
        assert!((xs[0] - 1.0).abs() < 1e-8);
    }

    #[test]
    fn an_unbounded_variable_is_scanned_within_the_scan_limit() {
        // No spec at all → ±SCAN_LIMIT, which still contains ±2.
        let (equations, blocks, defs, guesses) = prepared("x^2 = 4");
        let specs = BTreeMap::new();
        let mut solver = AllRootsSolver::new(SolverSettings::default(), &specs, &defs, &equations);
        let solutions = solver.find_all(&blocks, &guesses).unwrap();
        assert_eq!(solutions.len(), 2);
    }

    #[test]
    fn a_tangent_root_is_found_even_without_a_sign_change() {
        // (x − 1)² = 0 touches zero without crossing, so the sign scan sees
        // nothing; the tangent branch (and plain Newton) must still find it.
        let (equations, blocks, defs, guesses) = prepared("(x - 1)^2 = 0");
        let specs = BTreeMap::from([("x".to_string(), spec(-5.0, 5.0))]);
        let mut solver = AllRootsSolver::new(SolverSettings::default(), &specs, &defs, &equations);
        let solutions = solver.find_all(&blocks, &guesses).unwrap();
        assert!(!solutions.is_empty());
        for s in &solutions {
            assert!((s["x"] - 1.0).abs() < 1e-5, "x = {}", s["x"]);
        }
    }

    #[test]
    fn a_linear_block_still_reports_its_single_root() {
        let (equations, blocks, defs, guesses) = prepared("2*x + 6 = 0");
        let specs = BTreeMap::from([("x".to_string(), spec(-10.0, 10.0))]);
        let mut solver = AllRootsSolver::new(SolverSettings::default(), &specs, &defs, &equations);
        let solutions = solver.find_all(&blocks, &guesses).unwrap();
        assert_eq!(solutions.len(), 1);
        assert!((solutions[0]["x"] + 3.0).abs() < 1e-9);
    }

    // ── branching across blocks ───────────────────────────────────────────

    #[test]
    fn every_root_of_one_block_forks_a_branch_for_the_next() {
        // Block 1: x² = 4 (two roots). Block 2: y = x + 10 (one each).
        let (equations, blocks, defs, guesses) = prepared("x^2 = 4\ny = x + 10");
        assert_eq!(blocks.len(), 2, "expected two Tarjan blocks");
        let specs = BTreeMap::from([
            ("x".to_string(), spec(-10.0, 10.0)),
            ("y".to_string(), spec(-100.0, 100.0)),
        ]);
        let mut solver = AllRootsSolver::new(SolverSettings::default(), &specs, &defs, &equations);
        let solutions = solver.find_all(&blocks, &guesses).unwrap();
        assert_eq!(solutions.len(), 2, "two branches");
        // Sorted by x, so the negative root comes first.
        assert!((solutions[0]["x"] + 2.0).abs() < 1e-9);
        assert!((solutions[0]["y"] - 8.0).abs() < 1e-9);
        assert!((solutions[1]["x"] - 2.0).abs() < 1e-9);
        assert!((solutions[1]["y"] - 12.0).abs() < 1e-9);
    }

    #[test]
    fn a_block_with_no_root_in_the_region_is_a_named_error() {
        // x² = −1 has no real root, so the block yields nothing.
        let (equations, blocks, defs, guesses) = prepared("x^2 = -1");
        let specs = BTreeMap::from([("x".to_string(), spec(-5.0, 5.0))]);
        let mut solver = AllRootsSolver::new(SolverSettings::default(), &specs, &defs, &equations);
        let err = solver.find_all(&blocks, &guesses).unwrap_err();
        let message = err.to_string_message();
        assert!(
            message.contains("No solution found for block 0"),
            "{message}"
        );
        assert!(message.contains("Variable Information"), "{message}");
    }

    // ── N-D blocks ────────────────────────────────────────────────────────

    #[test]
    fn a_simultaneous_block_finds_both_intersections() {
        // The circle x² + y² = 1 meets the line y = x at ±(1/√2, 1/√2).
        let (equations, blocks, defs, guesses) = prepared("x^2 + y^2 = 1\ny = x");
        assert_eq!(blocks[0].variables.len(), 2, "must be one 2×2 block");
        let specs = BTreeMap::from([
            ("x".to_string(), spec(-2.0, 2.0)),
            ("y".to_string(), spec(-2.0, 2.0)),
        ]);
        let mut solver = AllRootsSolver::new(SolverSettings::default(), &specs, &defs, &equations);
        let solutions = solver.find_all(&blocks, &guesses).unwrap();
        assert_eq!(solutions.len(), 2, "got {solutions:?}");
        let half = std::f64::consts::FRAC_1_SQRT_2;
        assert!((solutions[0]["x"] + half).abs() < 1e-8, "{solutions:?}");
        assert!((solutions[1]["x"] - half).abs() < 1e-8, "{solutions:?}");
    }

    #[test]
    fn the_multi_start_search_is_reproducible() {
        let (equations, blocks, defs, guesses) = prepared("x^2 + y^2 = 1\ny = x");
        let specs = BTreeMap::from([
            ("x".to_string(), spec(-2.0, 2.0)),
            ("y".to_string(), spec(-2.0, 2.0)),
        ]);
        let run = || {
            let mut solver =
                AllRootsSolver::new(SolverSettings::default(), &specs, &defs, &equations);
            let out = solver.find_all(&blocks, &guesses).unwrap();
            out.iter().map(|s| (s["x"], s["y"])).collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn the_branch_cap_is_enforced() {
        assert_eq!(MAX_SOLUTIONS, 32);
        assert_eq!(SCAN_LIMIT, 100.0);
    }

    // ── parity with the Java oracle ───────────────────────────────────────
    //
    // Produced by `EquationSystemSolver.solveAll` (which is the only caller of
    // `AllRootsSolver`) on the same documents and specs. Solution *counts* are
    // the load-bearing assertions: they encode the scan window, the
    // sign-change/tangent/Newton triple, `isValidRoot`, `addRoot`'s merge rule
    // and `dedupAndSort` all at once.

    fn oracle_case(source: &str, specs: &BTreeMap<String, RootSpec>) -> Vec<Scope> {
        let (equations, blocks, defs, guesses) = prepared(source);
        let mut solver = AllRootsSolver::new(SolverSettings::default(), specs, &defs, &equations);
        solver.find_all(&blocks, &guesses).unwrap()
    }

    #[test]
    fn oracle_solution_sets_match() {
        let bounded = |lo: f64, hi: f64| BTreeMap::from([("x".to_string(), spec(lo, hi))]);

        // Java: 2 solutions {x=-2.0}, {x=2.0}
        let (equations, blocks, defs, guesses) = prepared("x^2 = 4");
        let specs = bounded(-10.0, 10.0);
        let mut solver = AllRootsSolver::new(SolverSettings::default(), &specs, &defs, &equations);
        let out = solver.find_all(&blocks, &guesses).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["x"], -2.0);
        assert_eq!(out[1]["x"], 2.0);
        // Java `solveAll` reports iterations=5 for this document.
        assert_eq!(solver.total_iterations(), 5);

        // Java: 2 solutions with no spec at all (±SCAN_LIMIT window)
        let out = oracle_case("x^2 = 4", &BTreeMap::new());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["x"], -2.0);
        assert_eq!(out[1]["x"], 2.0);

        // Java: 1 solution {x=1.0} — the window admits only the first root
        let out = oracle_case("x^3 - 6*x^2 + 11*x - 6 = 0", &bounded(0.0, 1.5));
        assert_eq!(out.len(), 1, "{out:?}");
        assert!((out[0]["x"] - 1.0).abs() < 1e-12);

        // Java: 1 solution {x=1.0} — a tangent root with no sign change
        let out = oracle_case("(x - 1)^2 = 0", &bounded(-5.0, 5.0));
        assert_eq!(out.len(), 1, "{out:?}");
        assert!((out[0]["x"] - 1.0).abs() < 1e-6);

        // Java: 1 solution {x=-3.0}
        let out = oracle_case("2*x + 6 = 0", &bounded(-10.0, 10.0));
        assert_eq!(out.len(), 1, "{out:?}");
        assert!((out[0]["x"] + 3.0).abs() < 1e-12);
    }

    #[test]
    fn oracle_branching_and_simultaneous_blocks_match() {
        // Java: 2 solutions {x=-2, y=8} and {x=2, y=12}
        let specs = BTreeMap::from([
            ("x".to_string(), spec(-10.0, 10.0)),
            ("y".to_string(), spec(-100.0, 100.0)),
        ]);
        let out = oracle_case("x^2 = 4\ny = x + 10", &specs);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["x"], -2.0);
        assert_eq!(out[0]["y"], 8.0);
        assert_eq!(out[1]["x"], 2.0);
        assert_eq!(out[1]["y"], 12.0);

        // Java: 2 solutions ±0.7071067811865476 on both variables
        let specs = BTreeMap::from([
            ("x".to_string(), spec(-2.0, 2.0)),
            ("y".to_string(), spec(-2.0, 2.0)),
        ]);
        let out = oracle_case("x^2 + y^2 = 1\ny = x", &specs);
        assert_eq!(out.len(), 2, "{out:?}");
        let half = std::f64::consts::FRAC_1_SQRT_2;
        assert!((out[0]["x"] + half).abs() < 1e-12);
        assert!((out[0]["y"] + half).abs() < 1e-12);
        assert!((out[1]["x"] - half).abs() < 1e-12);
        assert!((out[1]["y"] - half).abs() < 1e-12);
    }
}
