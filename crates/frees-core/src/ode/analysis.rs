//! Tolerant static analysis of a `DYNAMIC` block.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/core/ode/DynamicAnalysis.java`
//! (113 LOC).
//!
//! Reports a block's states, algebraic auxiliaries, output columns, and the set
//! of *input variables* it reads from the analytic system. Two callers need it:
//!
//! * [`crate::ode::accessors`] maps an ODE-table column name to its owning block.
//! * The engine wires the structural dependency between an accessor constraint
//!   and the analytic variables that feed the block, so Tarjan blocking and the
//!   Newton Jacobian see the coupling.
//!
//! # Deliberately weaker than [`crate::ode::dynamic`]
//!
//! Unlike `DynamicSolver` this never fails — it analyses best-effort, on the
//! *unexpanded* body, before the analytic solve has resolved anything. Three
//! consequences follow from the Java and are load-bearing:
//!
//! * `FOR` loops are walked but **not** expanded, so `der(T[i])` contributes the
//!   base name `t`, not `t[1] … t[N]`. Column ownership therefore has to match
//!   an array element against its base name — see
//!   [`crate::ode::accessors::owns_column`].
//! * [`der_state_name`] accepts an [`Expr::ArrayAccess`] argument where
//!   [`crate::ode::dynamic::der_state_name`] insists on a bare [`Expr::Var`];
//!   that asymmetry is exactly what makes the previous point work.
//! * A body equation whose left side is neither `der(…)` nor a simple name (an
//!   implicit constraint like `a.Qdot + b.Qdot = 0`) contributes no auxiliary
//!   here, though `DynamicSolver` will later register one.

use std::collections::BTreeSet;

use crate::ast::{Equation, Expr, Statement};
use crate::ode::dynamic::DynamicSystem;

/// The shape of one `DYNAMIC` block.
///
/// Port of `DynamicAnalysis.Shape`.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    /// Differentiated variables, in first-seen order.
    pub states: Vec<String>,
    /// Algebraic auxiliaries, in first-seen order.
    pub aux: Vec<String>,
    /// `[timeVar, states…, aux…]`.
    pub columns: Vec<String>,
    /// Everything the block reads that it does not itself define — the
    /// parameters and initial values the analytic system supplies. Sorted.
    pub input_vars: BTreeSet<String>,
}

/// Analyse a block. Port of `DynamicAnalysis.analyze`.
pub fn analyze(ds: &DynamicSystem) -> Shape {
    let mut states: Vec<String> = Vec::new();
    let mut aux: Vec<String> = Vec::new();
    let mut refs: BTreeSet<String> = BTreeSet::new();

    for eq in &ds.body_equations {
        classify(eq, &mut states, &mut aux);
        refs.extend(eq.lhs.variables());
        refs.extend(eq.rhs.variables());
    }
    for fb in &ds.for_blocks {
        collect_for(fb, &mut states, &mut aux, &mut refs);
    }
    for ic in &ds.initials {
        refs.extend(ic.value.variables());
    }
    for ev in &ds.events {
        refs.extend(ev.lhs.variables());
        refs.extend(ev.rhs.variables());
    }

    let time_var = &ds.options.time_var;
    let mut inputs = refs;
    for s in &states {
        inputs.remove(s);
    }
    for a in &aux {
        inputs.remove(a);
    }
    inputs.remove(time_var);

    let mut columns = Vec::with_capacity(1 + states.len() + aux.len());
    columns.push(time_var.clone());
    columns.extend(states.iter().cloned());
    columns.extend(aux.iter().cloned());
    Shape {
        states,
        aux,
        columns,
        input_vars: inputs,
    }
}

/// A `der(X) = …` left side makes `X` a state; a plain `name = …` left side
/// makes `name` an auxiliary; anything else contributes neither.
fn classify(eq: &Equation, states: &mut Vec<String>, aux: &mut Vec<String>) {
    if let Some(s) = der_state_name(&eq.lhs) {
        push_unique(states, s);
    } else if let Some(a) = simple_var(&eq.lhs) {
        push_unique(aux, a);
    }
}

/// Walk a `FOR` block's body. The loop variable is removed from `refs` on the
/// way out — it is bound by the loop, not supplied by the analytic system.
fn collect_for(
    block: &Statement,
    states: &mut Vec<String>,
    aux: &mut Vec<String>,
    refs: &mut BTreeSet<String>,
) {
    let Statement::For { var_name, body, .. } = block else {
        return;
    };
    let loop_var = var_name.to_ascii_lowercase();
    for st in body {
        match st {
            Statement::Eq(eq) => {
                classify(eq, states, aux);
                refs.extend(eq.lhs.variables());
                refs.extend(eq.rhs.variables());
            }
            inner @ Statement::For { .. } => collect_for(inner, states, aux, refs),
            _ => {}
        }
    }
    refs.remove(&loop_var);
}

/// If `lhs` is `der(X)` or `der(X[i])`, the *base* name `X`; otherwise `None`.
///
/// Port of `DynamicAnalysis.derStateName`. Accepting the array form is what lets
/// an accessor find the block owning `t[4]` before the discretization has been
/// expanded.
pub fn der_state_name(lhs: &Expr) -> Option<String> {
    match lhs {
        Expr::Call { function, args } if function == "der" && args.len() == 1 => match &args[0] {
            Expr::Var(name) => Some(name.clone()),
            Expr::ArrayAccess { name, .. } => Some(name.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// A bare variable or an array element's base name. Port of
/// `DynamicAnalysis.simpleVar`.
pub fn simple_var(lhs: &Expr) -> Option<String> {
    match lhs {
        Expr::Var(name) => Some(name.clone()),
        Expr::ArrayAccess { name, .. } => Some(name.clone()),
        _ => None,
    }
}

/// `LinkedHashSet.add` semantics over a `Vec`: append unless already present.
fn push_unique(out: &mut Vec<String>, name: String) {
    if !out.contains(&name) {
        out.push(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinOp;
    use crate::ode::dynamic::{DynamicOptions, InitialCondition};
    use crate::ode::events::DynamicEvent;

    fn eq(lhs: Expr, rhs: Expr) -> Equation {
        Equation::new(lhs, rhs, "src")
    }

    fn der(state: &str) -> Expr {
        Expr::call("der", vec![Expr::var(state)])
    }

    fn system(body: Vec<Equation>, for_blocks: Vec<Statement>) -> DynamicSystem {
        DynamicSystem {
            name: "b".into(),
            options: DynamicOptions::defaults("t", 0.0, 1.0),
            body_equations: body,
            for_blocks,
            initials: Vec::new(),
            events: Vec::new(),
            source_text: String::new(),
        }
    }

    #[test]
    fn states_auxiliaries_and_columns_come_out_in_body_order() {
        let ds = system(
            vec![
                eq(
                    der("temp"),
                    Expr::bin(BinOp::Div, Expr::var("qdot"), Expr::var("c")),
                ),
                eq(
                    Expr::var("qdot"),
                    Expr::bin(
                        BinOp::Mul,
                        Expr::var("k"),
                        Expr::bin(BinOp::Sub, Expr::var("tinf"), Expr::var("temp")),
                    ),
                ),
                eq(der("h"), Expr::var("v")),
            ],
            Vec::new(),
        );
        let shape = analyze(&ds);
        assert_eq!(shape.states, ["temp", "h"]);
        assert_eq!(shape.aux, ["qdot"]);
        assert_eq!(shape.columns, ["t", "temp", "h", "qdot"]);
        // Inputs are everything read but not defined; `t` is excluded.
        assert_eq!(
            shape.input_vars.iter().cloned().collect::<Vec<_>>(),
            ["c", "k", "tinf", "v"]
        );
    }

    #[test]
    fn the_time_variable_is_never_an_input() {
        let ds = system(vec![eq(der("x"), Expr::var("t"))], Vec::new());
        let shape = analyze(&ds);
        assert!(shape.input_vars.is_empty());
        assert_eq!(shape.columns, ["t", "x"]);
    }

    #[test]
    fn initial_and_event_expressions_contribute_inputs() {
        let mut ds = system(vec![eq(der("x"), Expr::num(1.0))], Vec::new());
        ds.initials.push(InitialCondition {
            state: "x".into(),
            indices: Vec::new(),
            value: Expr::var("x0"),
        });
        ds.events.push(DynamicEvent::new(
            "e",
            Expr::var("x"),
            Expr::var("limit"),
            None,
            "stop",
        ));
        let shape = analyze(&ds);
        assert_eq!(
            shape.input_vars.iter().cloned().collect::<Vec<_>>(),
            ["limit", "x0"]
        );
    }

    #[test]
    fn a_for_body_contributes_the_array_base_name_not_its_elements() {
        // FOR i = 1 TO n: der(T[i]) = (T[i+1] - T[i]) / dx
        let ds = system(
            Vec::new(),
            vec![Statement::For {
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
                    Expr::bin(
                        BinOp::Div,
                        Expr::ArrayAccess {
                            name: "t".into(),
                            indices: vec![Expr::var("i")],
                        },
                        Expr::var("dx"),
                    ),
                ))],
            }],
        );
        let shape = analyze(&ds);
        assert_eq!(shape.states, ["t"]);
        assert_eq!(shape.columns, ["t", "t"]);
        // `i` is bound by the loop and must not be an input; `n` is not read
        // inside the body, so the Java does not see it either.
        assert_eq!(shape.input_vars.iter().cloned().collect::<Vec<_>>(), ["dx"]);
    }

    #[test]
    fn a_nested_for_removes_both_loop_variables() {
        let ds = system(
            Vec::new(),
            vec![Statement::For {
                var_name: "i".into(),
                start: Expr::num(1.0),
                end: Expr::num(2.0),
                body: vec![Statement::For {
                    var_name: "J".into(),
                    start: Expr::num(1.0),
                    end: Expr::num(2.0),
                    body: vec![Statement::Eq(eq(
                        Expr::var("q"),
                        Expr::bin(
                            BinOp::Add,
                            Expr::bin(BinOp::Add, Expr::var("i"), Expr::var("j")),
                            Expr::var("g"),
                        ),
                    ))],
                }],
            }],
        );
        let shape = analyze(&ds);
        assert_eq!(shape.aux, ["q"]);
        assert_eq!(shape.input_vars.iter().cloned().collect::<Vec<_>>(), ["g"]);
    }

    #[test]
    fn an_implicit_constraint_contributes_no_auxiliary_here() {
        // The tolerant analysis only classifies `der(x) = …` and `name = …`.
        let ds = system(
            vec![eq(
                Expr::bin(BinOp::Add, Expr::var("a"), Expr::var("b")),
                Expr::num(0.0),
            )],
            Vec::new(),
        );
        let shape = analyze(&ds);
        assert!(shape.states.is_empty());
        assert!(shape.aux.is_empty());
        assert_eq!(shape.columns, ["t"]);
        assert_eq!(
            shape.input_vars.iter().cloned().collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[test]
    fn der_state_name_accepts_the_array_form_unlike_the_solver() {
        assert_eq!(der_state_name(&der("x")), Some("x".into()));
        assert_eq!(
            der_state_name(&Expr::call(
                "der",
                vec![Expr::ArrayAccess {
                    name: "t".into(),
                    indices: vec![Expr::var("i")]
                }]
            )),
            Some("t".into())
        );
        // The solver's stricter variant refuses the same expression.
        assert_eq!(
            crate::ode::dynamic::der_state_name(&Expr::call(
                "der",
                vec![Expr::ArrayAccess {
                    name: "t".into(),
                    indices: vec![Expr::var("i")]
                }]
            )),
            None
        );
        assert_eq!(der_state_name(&Expr::var("x")), None);
    }

    #[test]
    fn simple_var_reads_through_an_array_subscript() {
        assert_eq!(simple_var(&Expr::var("q")), Some("q".into()));
        assert_eq!(
            simple_var(&Expr::ArrayAccess {
                name: "q".into(),
                indices: vec![Expr::num(2.0)]
            }),
            Some("q".into())
        );
        assert_eq!(simple_var(&Expr::num(1.0)), None);
    }

    #[test]
    fn a_repeated_definition_is_listed_once() {
        let ds = system(
            vec![
                eq(Expr::var("q"), Expr::num(1.0)),
                eq(Expr::var("q"), Expr::num(2.0)),
                eq(der("x"), Expr::num(0.0)),
                eq(der("x"), Expr::num(1.0)),
            ],
            Vec::new(),
        );
        let shape = analyze(&ds);
        assert_eq!(shape.aux, ["q"]);
        assert_eq!(shape.states, ["x"]);
    }
}
