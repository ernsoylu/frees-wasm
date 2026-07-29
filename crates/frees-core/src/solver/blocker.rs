//! Decomposition of an equation system into sequentially solvable blocks.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/core/Blocker.java`
//! (384 LOC). Tarjan SCC comes from `petgraph` rather than JGraphT.
//!
//! # The algorithm
//!
//! 1. The unknowns are every variable mentioned by the system minus the caller's
//!    `knowns`. Degrees of freedom are `unknowns - equations`; anything but zero
//!    is a user error and is reported by naming the free quantities (or the
//!    redundant relations) rather than dumping counts.
//! 2. A **maximum bipartite matching** (Hopcroft–Karp, replacing JGraphT's
//!    `HopcroftKarpMaximumCardinalityBipartiteMatching`) assigns each equation
//!    the one unknown it will determine. A square system with no perfect
//!    matching is structurally singular: one part is overspecified while another
//!    is underspecified, and both parts are named.
//! 3. Edge `i -> j` in the equation digraph means "equation `i` reads the
//!    unknown equation `j` determines", i.e. `i` depends on `j`. Tarjan emits
//!    SCCs in reverse topological order — dependencies first — which is exactly
//!    the solve order, so the components come out already ordered.
//!
//! The blocks produced are the fine Dulmage–Mendelsohn decomposition and do not
//! depend on *which* perfect matching is found; only the relative order of two
//! mutually independent blocks can vary.

use crate::ast::Equation;
use crate::diag::{FreesError, Result};
use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

/// A group of equations that must be solved simultaneously, with the unknowns
/// they determine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Indices into the caller's equation slice.
    pub equations: Vec<usize>,
    /// Unknowns solved by this block, sorted.
    pub variables: Vec<String>,
}

impl Block {
    pub fn is_scalar(&self) -> bool {
        self.equations.len() == 1 && self.variables.len() == 1
    }
}

/// Result of decomposing a system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockingReport {
    /// Blocks in the order they must be solved.
    pub blocks: Vec<Block>,
}

/// Decompose `equations` into blocks, treating `knowns` as already determined.
///
/// Fails with a [`crate::diag::FreesError::Solver`] when the system has nonzero
/// degrees of freedom or no perfect equation↔variable matching exists — the
/// same condition `POST /api/check` reports without solving.
pub fn block_system(equations: &[Equation], knowns: &HashSet<String>) -> Result<BlockingReport> {
    // `Blocker.verifyStructure`: an empty document is a user error, not an
    // empty (trivially satisfied) solve.
    if equations.is_empty() {
        return Err(FreesError::solver("No equations to solve."));
    }

    let structure = Structure::new(equations, knowns);

    if structure.vars.len() != equations.len() {
        return Err(FreesError::solver(structure.causality_diagnosis()));
    }

    let matching = structure.maximum_matching();
    if matching.size != equations.len() {
        return Err(FreesError::solver(format!(
            "The equation system is structurally singular: no complete assignment of \
             equations to variables exists. {}",
            structure.causality_diagnosis()
        )));
    }

    Ok(BlockingReport {
        blocks: structure.tarjan_blocks(&matching),
    })
}

/// Degrees of freedom: `unknowns - equations`. Zero is required to solve.
pub fn degrees_of_freedom(equations: &[Equation], knowns: &HashSet<String>) -> i64 {
    unknowns(equations, knowns).len() as i64 - equations.len() as i64
}

/// All variables appearing in the system that are not in `knowns`.
///
/// Sorted, deduplicated and lowercase (frees identifiers are case-insensitive,
/// so `knowns` is compared case-insensitively too).
pub fn unknowns(equations: &[Equation], knowns: &HashSet<String>) -> Vec<String> {
    let known = normalized(knowns);
    let mut out = BTreeSet::new();
    for eq in equations {
        for v in eq.variables() {
            if !known.contains(&v) {
                out.insert(v);
            }
        }
    }
    out.into_iter().collect()
}

/// Scratch alias kept so implementations can build adjacency without importing.
pub type Adjacency = HashMap<usize, Vec<String>>;

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Sentinel for "no match" / "unreachable", used for both matching slots and
/// Hopcroft–Karp BFS distances.
const NIL: usize = usize::MAX;

fn normalized(knowns: &HashSet<String>) -> HashSet<String> {
    knowns.iter().map(|k| k.to_ascii_lowercase()).collect()
}

/// The bipartite incidence structure: equations on the left, unknowns on the
/// right, with unknowns identified by their index into `vars`.
struct Structure<'a> {
    equations: &'a [Equation],
    /// Sorted unknown names.
    vars: Vec<String>,
    /// `adjacency[i]` = ascending indices into `vars` used by equation `i`.
    /// Variables in `knowns` are absent — they are constants, not graph nodes.
    adjacency: Vec<Vec<usize>>,
}

impl<'a> Structure<'a> {
    fn new(equations: &'a [Equation], knowns: &HashSet<String>) -> Structure<'a> {
        let vars = unknowns(equations, knowns);
        let index: HashMap<&str, usize> = vars
            .iter()
            .enumerate()
            .map(|(i, v)| (v.as_str(), i))
            .collect();
        let adjacency = equations
            .iter()
            .map(|eq| {
                let mut row: Vec<usize> = eq
                    .variables()
                    .iter()
                    .filter_map(|v| index.get(v.as_str()).copied())
                    .collect();
                row.sort_unstable();
                row
            })
            .collect();
        Structure {
            equations,
            vars,
            adjacency,
        }
    }

    /// `var -> equations that mention it`, mirroring `variableUsageIndex`.
    fn variable_usage(&self) -> Vec<Vec<usize>> {
        let mut usage = vec![Vec::new(); self.vars.len()];
        for (i, row) in self.adjacency.iter().enumerate() {
            for &v in row {
                usage[v].push(i);
            }
        }
        usage
    }

    // -- matching ----------------------------------------------------------

    /// Hopcroft–Karp maximum-cardinality bipartite matching.
    ///
    /// Deterministic: adjacency rows are sorted and phases sweep equations in
    /// index order. Iterative throughout — real systems reach thousands of
    /// equations and the Java port is iterative for the same reason.
    fn maximum_matching(&self) -> Matching {
        let n = self.adjacency.len();
        let m = self.vars.len();
        let mut eq_to_var = vec![NIL; n];
        let mut var_to_eq = vec![NIL; m];
        let mut dist = vec![NIL; n];
        let mut queue: VecDeque<usize> = VecDeque::new();
        let mut size = 0usize;

        loop {
            // BFS: layer the free equations, looking for a free variable.
            queue.clear();
            let mut dist_nil = NIL;
            for u in 0..n {
                if eq_to_var[u] == NIL {
                    dist[u] = 0;
                    queue.push_back(u);
                } else {
                    dist[u] = NIL;
                }
            }
            while let Some(u) = queue.pop_front() {
                if dist[u] >= dist_nil {
                    continue;
                }
                for &v in &self.adjacency[u] {
                    let w = var_to_eq[v];
                    if w == NIL {
                        if dist_nil == NIL {
                            dist_nil = dist[u] + 1;
                        }
                    } else if dist[w] == NIL {
                        dist[w] = dist[u] + 1;
                        queue.push_back(w);
                    }
                }
            }
            if dist_nil == NIL {
                break; // no augmenting path left: the matching is maximum
            }

            // DFS along the shortest augmenting paths only.
            for u in 0..n {
                if eq_to_var[u] == NIL
                    && self.augment(u, dist_nil, &mut dist, &mut eq_to_var, &mut var_to_eq)
                {
                    size += 1;
                }
            }
        }

        Matching {
            eq_to_var,
            var_to_eq,
            size,
        }
    }

    /// One iterative layered DFS. `stack` frames are `(equation, next edge)`;
    /// the edge already taken by frame `k` is `adjacency[u][next - 1]`.
    fn augment(
        &self,
        start: usize,
        dist_nil: usize,
        dist: &mut [usize],
        eq_to_var: &mut [usize],
        var_to_eq: &mut [usize],
    ) -> bool {
        let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
        while let Some(&(u, next)) = stack.last() {
            let row = &self.adjacency[u];
            if next >= row.len() {
                dist[u] = NIL; // exhausted: never revisit in this phase
                stack.pop();
                continue;
            }
            let v = row[next];
            stack.last_mut().expect("non-empty stack").1 += 1;
            let w = var_to_eq[v];
            let layer = if w == NIL { dist_nil } else { dist[w] };
            if dist[u] == NIL || layer == NIL || layer != dist[u] + 1 {
                continue;
            }
            if w == NIL {
                // Free variable reached: flip every edge on the path.
                for &(eq, taken) in stack.iter() {
                    let var = self.adjacency[eq][taken - 1];
                    eq_to_var[eq] = var;
                    var_to_eq[var] = eq;
                }
                return true;
            }
            stack.push((w, 0));
        }
        false
    }

    // -- blocking ----------------------------------------------------------

    /// Tarjan SCC over the equation dependency digraph.
    ///
    /// Edge `i -> j` means equation `i` reads the unknown equation `j`
    /// determines. `tarjan_scc` returns components in postorder — reverse
    /// topological order — so dependencies come out first and the returned
    /// vector is already the solve order.
    fn tarjan_blocks(&self, matching: &Matching) -> Vec<Block> {
        let n = self.adjacency.len();
        let mut graph: DiGraph<(), ()> = DiGraph::with_capacity(n, n);
        let nodes: Vec<NodeIndex> = (0..n).map(|_| graph.add_node(())).collect();
        for (i, row) in self.adjacency.iter().enumerate() {
            for &v in row {
                let j = matching.var_to_eq[v];
                // NIL means the variable is unmatched (a free/external constant).
                if j != NIL && j != i {
                    graph.add_edge(nodes[i], nodes[j], ());
                }
            }
        }

        tarjan_scc(&graph)
            .into_iter()
            .map(|component| {
                let mut equations: Vec<usize> = component.iter().map(|node| node.index()).collect();
                equations.sort_unstable();
                let mut variables: Vec<String> = equations
                    .iter()
                    .filter_map(|&i| match matching.eq_to_var[i] {
                        NIL => None,
                        v => Some(self.vars[v].clone()),
                    })
                    .collect();
                variables.sort();
                Block {
                    equations,
                    variables,
                }
            })
            .collect()
    }

    // -- diagnosis ---------------------------------------------------------

    /// Names the causality hole instead of dumping counts: runs the maximum
    /// matching anyway and reports the exact unmatched variables (quantities no
    /// equation determines) plus the coupled underdetermined family, or
    /// symmetrically the unmatched — redundant — equations, quoted from the
    /// user's own source text.
    fn causality_diagnosis(&self) -> String {
        let matching = self.maximum_matching();
        let n = self.equations.len();
        let m = self.vars.len();
        let mut sb = String::new();
        if n == m {
            // Square but singular: name *both* sides, since one part is
            // overspecified while another is underspecified.
            sb.push_str(&format!(
                "There are {n} equations and {m} variables — the system is square, but part of \
                 it is overspecified while another part is underspecified."
            ));
            self.append_redundant_relations(&mut sb, &matching, false);
            self.append_free_quantities(&mut sb, &matching, false);
            sb.push_str(
                " A common cause: the same physics stated twice for one quantity while another \
                 is left with no defining relation — a boundary pinning a value a component \
                 already defines, or an element chain missing a constitutive law.",
            );
        } else {
            let under = n < m;
            sb.push_str(&format!(
                "There are {n} equations and {m} variables. The problem is {} and cannot be \
                 solved.",
                if under {
                    "underspecified"
                } else {
                    "overspecified"
                }
            ));
            if under {
                self.append_free_quantities(&mut sb, &matching, true);
            } else {
                self.append_redundant_relations(&mut sb, &matching, true);
            }
        }
        sb
    }

    fn append_free_quantities(&self, sb: &mut String, matching: &Matching, hint: bool) {
        let free_flat: Vec<usize> = (0..self.vars.len())
            .filter(|&v| matching.var_to_eq[v] == NIL)
            .collect();
        if free_flat.is_empty() {
            return;
        }
        let free: Vec<String> = free_flat
            .iter()
            .map(|&v| display_name(&self.vars[v]))
            .collect();
        sb.push_str(" Free quantit");
        sb.push_str(if free.len() == 1 { "y" } else { "ies" });
        sb.push_str(" (no defining relation): ");
        sb.push_str(&limit(&free, 8));
        sb.push('.');

        let mut group = self.alternating_reachable(matching);
        for v in &free_flat {
            group.remove(v);
        }
        if !group.is_empty() {
            let shown: Vec<String> = group
                .iter()
                .take(12)
                .map(|&v| display_name(&self.vars[v]))
                .collect();
            sb.push_str(" Coupled to: ");
            sb.push_str(&shown.join(", "));
            if group.len() > 12 {
                sb.push_str(", …");
            }
            sb.push('.');
        }
        if hint {
            sb.push_str(
                " A common cause: an element chain with no constitutive law for that quantity \
                 — e.g. an efficiency-only machine or rigid pass-through between boundaries \
                 leaves its through-flow or a port pressure free; add an orifice/valve/flow-map \
                 element or pin a boundary value.",
            );
        }
    }

    fn append_redundant_relations(&self, sb: &mut String, matching: &Matching, hint: bool) {
        let redundant: Vec<&str> = (0..self.equations.len())
            .filter(|&i| matching.eq_to_var[i] == NIL)
            .take(4)
            .map(|i| self.equations[i].source_text.as_str())
            .collect();
        if redundant.is_empty() {
            return;
        }
        sb.push_str(" Redundant relation");
        if redundant.len() != 1 {
            sb.push('s');
        }
        sb.push_str(" (no free variable left to determine): ");
        sb.push_str(&redundant.join("; "));
        sb.push('.');
        if hint {
            sb.push_str(
                " A common cause: the same physics stated twice — a boundary pinning a quantity \
                 a component already defines (a re-equated mixer pressure, a T-pinned wall \
                 state), or two property calls restating one relation.",
            );
        }
    }

    /// Variables reachable from the unmatched variables via alternating paths
    /// (var → any equation using it → that equation's matched variable → …):
    /// the whole structurally-underdetermined family.
    fn alternating_reachable(&self, matching: &Matching) -> BTreeSet<usize> {
        let usage = self.variable_usage();
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        let mut frontier: VecDeque<usize> = VecDeque::new();
        for (v, eqs) in usage.iter().enumerate() {
            if matching.var_to_eq[v] == NIL && !eqs.is_empty() {
                seen.insert(v);
                frontier.push_back(v);
            }
        }
        let mut seen_eq = vec![false; self.equations.len()];
        while let Some(v) = frontier.pop_front() {
            for &eq in &usage[v] {
                if seen_eq[eq] {
                    continue;
                }
                seen_eq[eq] = true;
                let matched = matching.eq_to_var[eq];
                if matched != NIL && seen.insert(matched) {
                    frontier.push_back(matched);
                }
            }
        }
        seen
    }
}

/// A maximum matching plus its size, for diagnosis.
struct Matching {
    /// `eq_to_var[i]` = variable index equation `i` determines, or [`NIL`].
    eq_to_var: Vec<usize>,
    /// `var_to_eq[v]` = equation determining variable `v`, or [`NIL`].
    var_to_eq: Vec<usize>,
    size: usize,
}

/// `$` separates a component member from its owner internally; the user wrote a
/// dot. Diagnostics are source-mapped, never mangled.
fn display_name(v: &str) -> String {
    v.replace('$', ".")
}

fn limit(items: &[String], n: usize) -> String {
    let head = items
        .iter()
        .take(n)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > n {
        format!("{head}, …")
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BinOp, Expr};

    // -- fixtures ----------------------------------------------------------

    /// An equation whose variable set is exactly `vars`, carrying `source` as
    /// its verbatim text. The shape (`vars[0] = 1 + vars[1] + …`) is irrelevant
    /// to blocking — only the incidence pattern matters.
    fn eqn(source: &str, vars: &[&str]) -> Equation {
        let lhs = match vars.first() {
            Some(v) => Expr::var(v),
            None => Expr::num(1.0),
        };
        let rhs = vars.iter().skip(1).fold(Expr::num(1.0), |acc, v| {
            Expr::bin(BinOp::Add, acc, Expr::var(v))
        });
        Equation::new(lhs, rhs, source)
    }

    fn known(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn none() -> HashSet<String> {
        HashSet::new()
    }

    fn vars_of(report: &BlockingReport) -> Vec<Vec<String>> {
        report.blocks.iter().map(|b| b.variables.clone()).collect()
    }

    fn eqs_of(report: &BlockingReport) -> Vec<Vec<usize>> {
        report.blocks.iter().map(|b| b.equations.clone()).collect()
    }

    fn err_message(equations: &[Equation], knowns: &HashSet<String>) -> String {
        match block_system(equations, knowns) {
            Err(FreesError::Solver { message }) => message,
            other => panic!("expected a solver error, got {other:?}"),
        }
    }

    /// The invariant the whole module exists to guarantee: every equation and
    /// every unknown is placed exactly once, and no block reads an unknown that
    /// a *later* block determines.
    fn assert_solve_order(
        equations: &[Equation],
        knowns: &HashSet<String>,
        report: &BlockingReport,
    ) {
        let mut owner: HashMap<String, usize> = HashMap::new();
        for (bi, block) in report.blocks.iter().enumerate() {
            assert_eq!(
                block.equations.len(),
                block.variables.len(),
                "block {bi} is not square: {block:?}"
            );
            for v in &block.variables {
                assert!(
                    owner.insert(v.clone(), bi).is_none(),
                    "variable {v} determined by two blocks"
                );
            }
        }
        let expected: Vec<String> = unknowns(equations, knowns);
        assert_eq!(
            owner.len(),
            expected.len(),
            "not every unknown is determined"
        );
        for v in &expected {
            assert!(owner.contains_key(v), "unknown {v} determined by no block");
        }

        let mut placed = vec![false; equations.len()];
        for block in &report.blocks {
            for &i in &block.equations {
                assert!(!placed[i], "equation {i} placed twice");
                placed[i] = true;
            }
        }
        assert!(placed.iter().all(|&p| p), "some equation was never placed");

        let knowns = normalized(knowns);
        for (bi, block) in report.blocks.iter().enumerate() {
            // A block may only claim unknowns its own equations mention —
            // i.e. the matching underneath is a real matching.
            let reachable: BTreeSet<String> = block
                .equations
                .iter()
                .flat_map(|&i| equations[i].variables())
                .collect();
            for v in &block.variables {
                assert!(
                    reachable.contains(v),
                    "block {bi} claims {v}, which none of its equations mention"
                );
            }
            for &i in &block.equations {
                for v in equations[i].variables() {
                    if knowns.contains(&v) {
                        continue;
                    }
                    let determined_in = owner[&v];
                    assert!(
                        determined_in <= bi,
                        "block {bi} (equation {i}: {}) reads {v}, determined later in block {determined_in}",
                        equations[i].source_text
                    );
                }
            }
        }
    }

    // -- unknowns / degrees of freedom -------------------------------------

    #[test]
    fn unknowns_are_sorted_deduplicated_and_minus_knowns() {
        let system = [
            eqn("z = y + x", &["z", "y", "x"]),
            eqn("y = x", &["y", "x"]),
        ];
        assert_eq!(unknowns(&system, &none()), vec!["x", "y", "z"]);
        assert_eq!(unknowns(&system, &known(&["y"])), vec!["x", "z"]);
        assert_eq!(
            unknowns(&system, &known(&["x", "y", "z"])),
            Vec::<String>::new()
        );
    }

    #[test]
    fn knowns_are_matched_case_insensitively() {
        let system = [eqn("y = x + 1", &["y", "x"])];
        // frees identifiers are case-insensitive and stored lowercase.
        assert_eq!(unknowns(&system, &known(&["X"])), vec!["y"]);
        assert_eq!(degrees_of_freedom(&system, &known(&["X"])), 0);
    }

    #[test]
    fn unknowns_of_an_empty_system_is_empty() {
        assert!(unknowns(&[], &none()).is_empty());
        assert_eq!(degrees_of_freedom(&[], &none()), 0);
    }

    #[test]
    fn degrees_of_freedom_counts_unknowns_minus_equations() {
        let under = [eqn("x + y = 1", &["x", "y"])];
        assert_eq!(degrees_of_freedom(&under, &none()), 1);
        assert_eq!(degrees_of_freedom(&under, &known(&["y"])), 0);

        let over = [eqn("x = 1", &["x"]), eqn("x = 2", &["x"])];
        assert_eq!(degrees_of_freedom(&over, &none()), -1);

        let square = [eqn("x = 1", &["x"]), eqn("y = x", &["y", "x"])];
        assert_eq!(degrees_of_freedom(&square, &none()), 0);
    }

    #[test]
    fn a_known_that_is_not_in_the_system_is_ignored() {
        let system = [eqn("x = 1", &["x"])];
        assert_eq!(unknowns(&system, &known(&["nowhere"])), vec!["x"]);
        assert_eq!(degrees_of_freedom(&system, &known(&["nowhere"])), 0);
    }

    // -- sequential systems -------------------------------------------------

    #[test]
    fn fully_sequential_system_is_scalar_blocks_in_dependency_order() {
        let system = [
            eqn("x = 1", &["x"]),
            eqn("y = x + 2", &["y", "x"]),
            eqn("z = y * 3", &["z", "y"]),
        ];
        let report = block_system(&system, &none()).expect("solvable");
        assert_solve_order(&system, &none(), &report);
        assert!(report.blocks.iter().all(Block::is_scalar));
        assert_eq!(vars_of(&report), vec![vec!["x"], vec!["y"], vec!["z"]]);
        assert_eq!(eqs_of(&report), vec![vec![0], vec![1], vec![2]]);
    }

    #[test]
    fn topological_order_beats_source_order() {
        // Written back to front: naive source order would solve z first, which
        // is catastrophically wrong. The blocker must reverse it.
        let system = [
            eqn("z = y * 3", &["z", "y"]),
            eqn("y = x + 2", &["y", "x"]),
            eqn("x = 1", &["x"]),
        ];
        let report = block_system(&system, &none()).expect("solvable");
        assert_solve_order(&system, &none(), &report);
        assert_eq!(eqs_of(&report), vec![vec![2], vec![1], vec![0]]);
        assert_eq!(vars_of(&report), vec![vec!["x"], vec!["y"], vec!["z"]]);
    }

    #[test]
    fn a_long_chain_stays_in_order_whatever_the_input_order() {
        // 0: e = d, 1: c = b, 2: a = 1, 3: d = c, 4: b = a
        let system = [
            eqn("e = d", &["e", "d"]),
            eqn("c = b", &["c", "b"]),
            eqn("a = 1", &["a"]),
            eqn("d = c", &["d", "c"]),
            eqn("b = a", &["b", "a"]),
        ];
        let report = block_system(&system, &none()).expect("solvable");
        assert_solve_order(&system, &none(), &report);
        assert_eq!(
            vars_of(&report),
            vec![vec!["a"], vec!["b"], vec!["c"], vec!["d"], vec!["e"]]
        );
    }

    #[test]
    fn knowns_break_a_cycle_into_scalar_blocks() {
        // Without knowns this is one 2x2 block; pinning x makes it sequential.
        let system = [
            eqn("y = x + 1", &["y", "x"]),
            eqn("z = y + x", &["z", "y", "x"]),
        ];
        let knowns = known(&["x"]);
        let report = block_system(&system, &knowns).expect("solvable");
        assert_solve_order(&system, &knowns, &report);
        assert_eq!(vars_of(&report), vec![vec!["y"], vec!["z"]]);
        assert!(report.blocks.iter().all(Block::is_scalar));
    }

    #[test]
    fn a_self_referential_equation_stays_a_scalar_block() {
        // x appears on both sides; the i -> i self edge must not merge blocks.
        let system = [eqn("x = x^2 + 1", &["x"]), eqn("y = x", &["y", "x"])];
        let report = block_system(&system, &none()).expect("solvable");
        assert_solve_order(&system, &none(), &report);
        assert_eq!(report.blocks.len(), 2);
        assert!(report.blocks.iter().all(Block::is_scalar));
        assert_eq!(vars_of(&report), vec![vec!["x"], vec!["y"]]);
    }

    // -- simultaneous systems ----------------------------------------------

    #[test]
    fn a_genuine_two_by_two_is_one_block() {
        let system = [eqn("x + y = 3", &["x", "y"]), eqn("x - y = 1", &["x", "y"])];
        let report = block_system(&system, &none()).expect("solvable");
        assert_solve_order(&system, &none(), &report);
        assert_eq!(report.blocks.len(), 1);
        assert!(!report.blocks[0].is_scalar());
        assert_eq!(report.blocks[0].equations, vec![0, 1]);
        assert_eq!(report.blocks[0].variables, vec!["x", "y"]);
    }

    #[test]
    fn a_genuine_three_by_three_is_one_block() {
        let system = [
            eqn("a + b + c = 6", &["a", "b", "c"]),
            eqn("a - b + c = 2", &["a", "b", "c"]),
            eqn("2*a + b - c = 1", &["a", "b", "c"]),
        ];
        let report = block_system(&system, &none()).expect("solvable");
        assert_solve_order(&system, &none(), &report);
        assert_eq!(report.blocks.len(), 1);
        assert_eq!(report.blocks[0].equations, vec![0, 1, 2]);
        assert_eq!(report.blocks[0].variables, vec!["a", "b", "c"]);
    }

    #[test]
    fn a_three_cycle_with_one_variable_each_is_still_one_block() {
        // a = f(c), b = f(a), c = f(b): pairwise sparse, globally coupled.
        let system = [
            eqn("a = c + 1", &["a", "c"]),
            eqn("b = a + 1", &["b", "a"]),
            eqn("c = b + 1", &["c", "b"]),
        ];
        let report = block_system(&system, &none()).expect("solvable");
        assert_solve_order(&system, &none(), &report);
        assert_eq!(report.blocks.len(), 1);
        assert_eq!(report.blocks[0].equations, vec![0, 1, 2]);
        assert_eq!(report.blocks[0].variables, vec!["a", "b", "c"]);
    }

    #[test]
    fn structurally_coupled_but_numerically_singular_still_blocks() {
        // Duplicate incidence pattern: perfect matching exists, so blocking
        // succeeds — detecting the rank deficiency is Newton's job, not ours.
        let system = [eqn("x + y = 3", &["x", "y"]), eqn("x + y = 4", &["x", "y"])];
        let report = block_system(&system, &none()).expect("structurally fine");
        assert_eq!(report.blocks.len(), 1);
        assert_eq!(report.blocks[0].variables, vec!["x", "y"]);
    }

    // -- mixed systems ------------------------------------------------------

    #[test]
    fn mixed_system_orders_scalar_and_simultaneous_blocks() {
        // Deliberately scrambled input order:
        //   0: w = x + y        (needs the 2x2)
        //   1: y = x - a        (2x2 with 3)
        //   2: a = 1            (scalar, first)
        //   3: x = y + a        (2x2 with 1)
        let system = [
            eqn("w = x + y", &["w", "x", "y"]),
            eqn("y = x - a", &["y", "x", "a"]),
            eqn("a = 1", &["a"]),
            eqn("x = y + a", &["x", "y", "a"]),
        ];
        let report = block_system(&system, &none()).expect("solvable");
        assert_solve_order(&system, &none(), &report);
        assert_eq!(report.blocks.len(), 3);
        assert_eq!(vars_of(&report), vec![vec!["a"], vec!["x", "y"], vec!["w"]]);
        assert_eq!(eqs_of(&report), vec![vec![2], vec![1, 3], vec![0]]);
        assert!(report.blocks[0].is_scalar());
        assert!(!report.blocks[1].is_scalar());
        assert!(report.blocks[2].is_scalar());
    }

    #[test]
    fn a_diamond_dependency_keeps_the_source_first_and_the_sink_last() {
        let system = [
            eqn("d = b + c", &["d", "b", "c"]),
            eqn("b = a * 2", &["b", "a"]),
            eqn("c = a * 3", &["c", "a"]),
            eqn("a = 1", &["a"]),
        ];
        let report = block_system(&system, &none()).expect("solvable");
        assert_solve_order(&system, &none(), &report);
        assert_eq!(report.blocks.len(), 4);
        assert_eq!(report.blocks[0].variables, vec!["a"]);
        assert_eq!(report.blocks[3].variables, vec!["d"]);
        // b and c are independent of each other; only their position relative
        // to a and d is fixed.
        let middle: BTreeSet<&str> = report.blocks[1..3]
            .iter()
            .flat_map(|b| b.variables.iter().map(String::as_str))
            .collect();
        assert_eq!(middle, ["b", "c"].into_iter().collect::<BTreeSet<_>>());
    }

    #[test]
    fn two_independent_subsystems_are_both_emitted_in_valid_order() {
        let system = [
            eqn("q = p", &["q", "p"]),
            eqn("n = m", &["n", "m"]),
            eqn("p = 1", &["p"]),
            eqn("m = 2", &["m"]),
        ];
        let report = block_system(&system, &none()).expect("solvable");
        assert_solve_order(&system, &none(), &report);
        assert_eq!(report.blocks.len(), 4);
    }

    #[test]
    fn blocking_is_deterministic() {
        let system = [
            eqn("w = x + y", &["w", "x", "y"]),
            eqn("y = x - a", &["y", "x", "a"]),
            eqn("a = 1", &["a"]),
            eqn("x = y + a", &["x", "y", "a"]),
        ];
        let first = block_system(&system, &none()).expect("solvable");
        for _ in 0..8 {
            assert_eq!(block_system(&system, &none()).expect("solvable"), first);
        }
    }

    // -- degrees-of-freedom errors -----------------------------------------

    #[test]
    fn empty_system_is_rejected() {
        assert_eq!(err_message(&[], &none()), "No equations to solve.");
    }

    #[test]
    fn underdetermined_system_names_the_free_quantities() {
        let system = [eqn("x + y = 1", &["x", "y"])];
        let message = err_message(&system, &none());
        assert!(
            message.starts_with(
                "There are 1 equations and 2 variables. The problem is underspecified and \
                 cannot be solved."
            ),
            "{message}"
        );
        assert!(
            message.contains("Free quantity (no defining relation): "),
            "{message}"
        );
        // Exactly one of x/y is free; the other is matched and merely coupled.
        assert!(message.contains("Coupled to: "), "{message}");
        assert!(message.contains('x') && message.contains('y'), "{message}");
        assert!(message.contains("A common cause:"), "{message}");
    }

    #[test]
    fn underdetermined_system_pluralises_and_truncates_free_quantities() {
        // One equation, ten variables: nine are free, so the list truncates at
        // eight with an ellipsis.
        let names: Vec<String> = (0..10).map(|i| format!("v{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let system = [eqn("v0 = v1 + v2", &refs)];
        let message = err_message(&system, &none());
        assert!(
            message.starts_with("There are 1 equations and 10 variables."),
            "{message}"
        );
        assert!(
            message.contains("Free quantities (no defining relation): "),
            "{message}"
        );
        assert!(message.contains(", …"), "{message}");
    }

    #[test]
    fn knowns_can_turn_an_underdetermined_system_solvable() {
        let system = [eqn("x + y = 1", &["x", "y"])];
        assert!(block_system(&system, &none()).is_err());
        let knowns = known(&["y"]);
        let report = block_system(&system, &knowns).expect("solvable once y is known");
        assert_eq!(vars_of(&report), vec![vec!["x"]]);
        assert_solve_order(&system, &knowns, &report);
    }

    #[test]
    fn overdetermined_system_quotes_the_redundant_source_lines() {
        let system = [eqn("x = 1", &["x"]), eqn("x = 2", &["x"])];
        let message = err_message(&system, &none());
        assert!(
            message.starts_with(
                "There are 2 equations and 1 variables. The problem is overspecified and cannot \
                 be solved."
            ),
            "{message}"
        );
        assert!(
            message.contains("Redundant relation (no free variable left to determine): x = 2."),
            "{message}"
        );
        assert!(message.contains("A common cause:"), "{message}");
    }

    #[test]
    fn overdetermined_system_lists_at_most_four_redundant_relations() {
        let mut system = vec![eqn("x = 0", &["x"])];
        for i in 1..8 {
            system.push(eqn(&format!("x = {i}"), &["x"]));
        }
        let message = err_message(&system, &none());
        assert!(message.contains("Redundant relations (no free variable left to determine):"));
        let listed = message.matches("x = ").count();
        assert_eq!(
            listed, 4,
            "at most four quoted lines, got {listed}: {message}"
        );
    }

    #[test]
    fn an_equation_over_only_knowns_makes_the_system_overspecified() {
        let system = [eqn("x = 1", &["x"]), eqn("x = t", &["x", "t"])];
        let knowns = known(&["t"]);
        let message = err_message(&system, &knowns);
        assert!(
            message.starts_with("There are 2 equations and 1 variables."),
            "{message}"
        );
    }

    // -- structural singularity --------------------------------------------

    #[test]
    fn structurally_singular_square_system_names_both_sides() {
        // 3 equations, 3 unknowns, but `x` is pinned twice while `z` has no
        // defining relation at all.
        let system = [
            eqn("x = 1", &["x"]),
            eqn("x = 2", &["x"]),
            eqn("y + z = 3", &["y", "z"]),
        ];
        assert_eq!(degrees_of_freedom(&system, &none()), 0);
        let message = err_message(&system, &none());
        assert!(
            message.starts_with(
                "The equation system is structurally singular: no complete assignment of \
                 equations to variables exists."
            ),
            "{message}"
        );
        assert!(
            message.contains(
                "There are 3 equations and 3 variables — the system is square, but part of it \
                 is overspecified while another part is underspecified."
            ),
            "{message}"
        );
        // Names the offending equation verbatim …
        assert!(
            message.contains("Redundant relation (no free variable left to determine): x = 2."),
            "{message}"
        );
        // … and the quantity nothing determines, plus the variable it is
        // coupled to through the shared equation.
        assert!(
            message.contains("Free quantity (no defining relation): z."),
            "{message}"
        );
        assert!(message.contains("Coupled to: y."), "{message}");
    }

    #[test]
    fn structurally_singular_system_with_two_holes() {
        // 4 equations / 4 unknowns: {p, q} are over-constrained by three
        // equations while `s` is determined by none.
        let system = [
            eqn("p = 1", &["p"]),
            eqn("q = 2", &["q"]),
            eqn("p + q = 3", &["p", "q"]),
            eqn("r + s = 4", &["r", "s"]),
        ];
        assert_eq!(degrees_of_freedom(&system, &none()), 0);
        let message = err_message(&system, &none());
        assert!(message.contains("structurally singular"), "{message}");
        assert!(
            message.contains("p + q = 3") || message.contains("p = 1") || message.contains("q = 2"),
            "{message}"
        );
        assert!(
            message.contains("Free quantity (no defining relation): s."),
            "{message}"
        );
    }

    #[test]
    fn a_variable_hidden_behind_knowns_can_make_the_system_singular() {
        // Both equations now see only `a`, so `b` is free and one relation is
        // redundant even though the counts still balance.
        let system = [
            eqn("a = t", &["a", "t"]),
            eqn("a = 2*t", &["a", "t"]),
            eqn("b + t = 1", &["b", "t"]),
        ];
        let knowns = known(&["t"]);
        assert_eq!(degrees_of_freedom(&system, &knowns), -1);
        let message = err_message(&system, &knowns);
        assert!(
            message.starts_with("There are 3 equations and 2 variables."),
            "{message}"
        );
    }

    #[test]
    fn display_names_unmangle_component_members() {
        let system = [eqn("pump$eta = 0.8", &["pump$eta", "pump$w"])];
        let message = err_message(&system, &none());
        assert!(
            message.contains("pump.eta") || message.contains("pump.w"),
            "{message}"
        );
        assert!(!message.contains("pump$"), "mangled name leaked: {message}");
    }

    // -- petgraph contract --------------------------------------------------

    #[test]
    fn tarjan_scc_returns_components_in_reverse_topological_order() {
        // Guards the single assumption the block order rests on: for an edge
        // a -> b, petgraph emits b's component before a's. With our edge
        // convention (i -> j == "i depends on j") that is the solve order.
        let mut graph: DiGraph<&str, ()> = DiGraph::new();
        let a = graph.add_node("a");
        let b = graph.add_node("b");
        let c = graph.add_node("c");
        graph.add_edge(a, b, ());
        graph.add_edge(b, c, ());
        let order: Vec<&str> = tarjan_scc(&graph)
            .into_iter()
            .map(|scc| graph[scc[0]])
            .collect();
        assert_eq!(order, vec!["c", "b", "a"]);
    }

    // -- matching cross-check ----------------------------------------------

    /// Reference maximum matching (textbook Kuhn's) to check Hopcroft–Karp
    /// against. Deliberately naive: correctness over speed.
    fn reference_matching(adjacency: &[Vec<usize>], n_vars: usize) -> usize {
        fn try_kuhn(
            u: usize,
            adjacency: &[Vec<usize>],
            visited: &mut [bool],
            var_to_eq: &mut [usize],
        ) -> bool {
            for &v in &adjacency[u] {
                if visited[v] {
                    continue;
                }
                visited[v] = true;
                if var_to_eq[v] == NIL || try_kuhn(var_to_eq[v], adjacency, visited, var_to_eq) {
                    var_to_eq[v] = u;
                    return true;
                }
            }
            false
        }

        let mut var_to_eq = vec![NIL; n_vars];
        let mut size = 0;
        for u in 0..adjacency.len() {
            let mut visited = vec![false; n_vars];
            if try_kuhn(u, adjacency, &mut visited, &mut var_to_eq) {
                size += 1;
            }
        }
        size
    }

    /// Deterministic xorshift, so a failure is always reproducible.
    fn next_random(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    #[test]
    fn hopcroft_karp_agrees_with_a_reference_maximum_matching() {
        let mut state = 0x2024_0731_dead_beefu64;
        for trial in 0..400 {
            let n_eq = 1 + (next_random(&mut state) % 9) as usize;
            let n_var = 1 + (next_random(&mut state) % 9) as usize;
            let mut adjacency: Vec<Vec<usize>> = Vec::with_capacity(n_eq);
            for _ in 0..n_eq {
                let mut row: Vec<usize> = (0..n_var)
                    .filter(|_| next_random(&mut state) % 3 != 0)
                    .collect();
                row.sort_unstable();
                adjacency.push(row);
            }
            let structure = Structure {
                equations: &[],
                vars: (0..n_var).map(|i| format!("v{i}")).collect(),
                adjacency: adjacency.clone(),
            };
            let matching = structure.maximum_matching();

            // Size matches the reference …
            assert_eq!(
                matching.size,
                reference_matching(&adjacency, n_var),
                "trial {trial}: {adjacency:?}"
            );
            // … and it really is a matching.
            let mut counted = 0;
            for (eq, &v) in matching.eq_to_var.iter().enumerate() {
                if v == NIL {
                    continue;
                }
                counted += 1;
                assert!(adjacency[eq].contains(&v), "trial {trial}: bogus pair");
                assert_eq!(matching.var_to_eq[v], eq, "trial {trial}: asymmetric pair");
            }
            assert_eq!(counted, matching.size, "trial {trial}: size disagrees");
            for (v, &eq) in matching.var_to_eq.iter().enumerate() {
                if eq != NIL {
                    assert_eq!(matching.eq_to_var[eq], v, "trial {trial}: asymmetric pair");
                }
            }
        }
    }

    #[test]
    fn random_square_systems_block_into_a_valid_solve_order() {
        let mut state = 0x5eed_0000_0f00_ba11u64;
        let mut solved = 0;
        for _ in 0..300 {
            let n = 2 + (next_random(&mut state) % 6) as usize;
            let names: Vec<String> = (0..n).map(|i| format!("x{i}")).collect();
            // Build an n x n system: every equation gets its "own" variable
            // (guaranteeing a perfect matching) plus random extra couplings.
            let system: Vec<Equation> = (0..n)
                .map(|i| {
                    let mut vars: Vec<&str> = vec![names[i].as_str()];
                    for (j, name) in names.iter().enumerate() {
                        if j != i && next_random(&mut state) % 4 == 0 {
                            vars.push(name.as_str());
                        }
                    }
                    eqn(&format!("equation {i}"), &vars)
                })
                .collect();
            let report = block_system(&system, &none()).expect("perfect matching exists");
            assert_solve_order(&system, &none(), &report);
            solved += 1;
        }
        assert_eq!(solved, 300);
    }

    // -- scale --------------------------------------------------------------

    #[test]
    fn a_long_reversed_chain_blocks_without_recursion_blowup() {
        // 2000 equations written in exactly the wrong order.
        let n = 2000usize;
        let names: Vec<String> = (0..n).map(|i| format!("v{i}")).collect();
        let mut system = Vec::with_capacity(n);
        for i in (1..n).rev() {
            system.push(eqn(
                &format!("v{i} = v{}", i - 1),
                &[&names[i], &names[i - 1]],
            ));
        }
        system.push(eqn("v0 = 1", &[&names[0]]));

        let report = block_system(&system, &none()).expect("solvable");
        assert_eq!(report.blocks.len(), n);
        assert!(report.blocks.iter().all(Block::is_scalar));
        assert_eq!(report.blocks[0].variables, vec!["v0"]);
        assert_solve_order(&system, &none(), &report);
    }

    #[test]
    fn a_large_simultaneous_ring_is_a_single_block() {
        let n = 300usize;
        let names: Vec<String> = (0..n).map(|i| format!("r{i}")).collect();
        let system: Vec<Equation> = (0..n)
            .map(|i| {
                let next = (i + 1) % n;
                eqn(&format!("r{i} = r{next} + 1"), &[&names[i], &names[next]])
            })
            .collect();
        let report = block_system(&system, &none()).expect("solvable");
        assert_eq!(report.blocks.len(), 1);
        assert_eq!(report.blocks[0].equations.len(), n);
        assert_eq!(report.blocks[0].variables.len(), n);
    }
}
