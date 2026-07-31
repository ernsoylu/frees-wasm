//! `EVENT` declarations: from the block's source form to the switching
//! functions the driver watches.
//!
//! Port of the event half of
//! `../frEES/backend/core/src/main/java/com/frees/backend/core/ode/OdeEvent.java`
//! and of `ast/DynamicSystem.Event`, plus the two places `DynamicSolver` turns
//! those records into something executable (`buildEvents` for the explicit
//! integrators, `buildRootFn` + `idaEventDirs`/`idaEventSetIdx`/`idaEventSetExpr`
//! for the IDA path).
//!
//! # The runtime `OdeEvent` is not defined here
//!
//! [`crate::ode::problem`] owns [`OdeEvent`] because `OdeProblem` structurally
//! contains a list of them; this module re-exports it. What lives here is the
//! *document* level: the parsed [`DynamicEvent`] record and its de-sugaring into
//! an [`EventBinding`].
//!
//! # Why `EventBinding` exists
//!
//! The Java builds the same four things twice — once in `DynamicSolver.buildEvents`
//! for the explicit integrators and once in `DynamicSolver.assembleDae` for IDA:
//! the `der()`-substituted switching expressions, the direction code, the
//! stop flag, and the resolved `set` state index. Both paths must agree
//! (a `set` target that is not a state has to be rejected identically, and the
//! IDA root handler explicitly says it honours the same rising/falling filter as
//! the built-in integrators). Binding once and consuming twice is the same
//! semantics with one source of truth.

use crate::ast::Expr;
use crate::diag::{FreesError, Result};

pub use crate::ode::problem::{direction_from_keyword, OdeEvent, OdeScalarFn, StateReset};

/// A zero-crossing event as written in a `DYNAMIC` block:
/// `EVENT name: lhs = rhs [| rising|falling] -> stop|record|set X = expr`.
///
/// Port of `ast/DynamicSystem.Event`. The crossing variable is `lhs - rhs`;
/// `direction` is one of `rising` / `falling` / `any` (absent means `any`) and
/// `action` one of `stop` / `record` / `set`.
///
/// A `set` action reassigns state [`set_var`](DynamicEvent::set_var) to
/// [`set_expr`](DynamicEvent::set_expr) at the crossing and **restarts
/// integration from the modified state** — the discrete latch that gives the
/// language true hysteresis.
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicEvent {
    /// The name written after `EVENT`. Unlike almost every other identifier in
    /// the language this keeps its **source case** — `AstBuilder.buildDynamicEvent`
    /// reads `ctx.IDENT(0).getText()` raw, while it lowercases the direction, the
    /// action and the `set` target — and the recorded hit carries it verbatim.
    pub name: String,
    /// Left side of the crossing equation.
    pub lhs: Expr,
    /// Right side of the crossing equation.
    pub rhs: Expr,
    /// `rising` / `falling` / `any`. The Java field is a nullable `String` whose
    /// `null` is this `None`; both mean "any".
    pub direction: Option<String>,
    /// `stop` / `record` / `set`, lowercase.
    pub action: String,
    /// For a `set` action, the state being reassigned (lowercase).
    pub set_var: Option<String>,
    /// For a `set` action, the expression giving the new value.
    pub set_expr: Option<Expr>,
}

impl DynamicEvent {
    /// stop/record event (no state reassignment) — the Java 5-argument
    /// convenience constructor.
    pub fn new(
        name: impl Into<String>,
        lhs: Expr,
        rhs: Expr,
        direction: Option<String>,
        action: impl Into<String>,
    ) -> DynamicEvent {
        DynamicEvent {
            name: name.into(),
            lhs,
            rhs,
            direction,
            action: action.into(),
            set_var: None,
            set_expr: None,
        }
    }

    /// Java `"stop".equals(ev.action())`.
    pub fn stops(&self) -> bool {
        self.action == "stop"
    }

    /// Java `"set".equals(ev.action())`.
    pub fn is_set(&self) -> bool {
        self.action == "set"
    }

    /// `+1` rising, `−1` falling, `0` any — `OdeEvent.directionFromKeyword`.
    pub fn direction_code(&self) -> i32 {
        direction_from_keyword(self.direction.as_deref())
    }
}

/// One [`DynamicEvent`] de-sugared against a classified block: switching
/// expressions with every `der(X)` rewritten to the reified unknown `der$X`, the
/// numeric direction code, the stop flag, and the resolved index of the state a
/// `set` action overwrites.
///
/// This is exactly the information both the explicit-integrator path and the
/// IDA path need; neither re-derives it.
#[derive(Debug, Clone, PartialEq)]
pub struct EventBinding {
    /// The event's name, carried through to the recorded hit.
    pub name: String,
    /// `lhs` with `der()` reified.
    pub lhs: Expr,
    /// `rhs` with `der()` reified.
    pub rhs: Expr,
    /// `+1` rising, `−1` falling, `0` any.
    pub direction: i32,
    /// Whether the crossing terminates the integration.
    pub stop: bool,
    /// Index into the state vector for a `set` action; `None` otherwise. The
    /// Java spells the absent case as `-1`.
    pub set_index: Option<usize>,
    /// The `set` action's replacement expression, with `der()` reified.
    pub set_expr: Option<Expr>,
}

/// De-sugar every event of a classified block.
///
/// `substitute_der` is `DynamicSolver.substituteDer` and `state_index` is the
/// position of a name in the block's state vector (`states.indexOf`). A `set`
/// action whose target is not a state is rejected here with the Java
/// `eventSetStateIndex` message — only differential states can be reassigned,
/// because the auxiliaries re-derive from them at the restart.
pub fn bind_events(
    events: &[DynamicEvent],
    substitute_der: impl Fn(&Expr) -> Expr,
    state_index: impl Fn(&str) -> Option<usize>,
) -> Result<Vec<EventBinding>> {
    let mut out = Vec::with_capacity(events.len());
    for ev in events {
        let (set_index, set_expr) = if ev.is_set() {
            let target = ev.set_var.as_deref().unwrap_or("");
            let idx = state_index(target).ok_or_else(|| {
                FreesError::solver(format!(
                    "EVENT {}: set target '{target}' is not a state of this DYNAMIC block \
                     — only der(...) states can be reassigned at an event.",
                    ev.name
                ))
            })?;
            // A `set` with no expression cannot happen through the grammar
            // (`ARROW IDENT (IDENT EQ expr)?` binds them together), but the AST
            // permits it; treating it as "assign zero" would silently corrupt a
            // trajectory, so refuse instead.
            let expr = ev.set_expr.as_ref().ok_or_else(|| {
                FreesError::solver(format!(
                    "EVENT {}: a `set` action needs a value — write \
                     `-> set {target} = <expr>`.",
                    ev.name
                ))
            })?;
            (Some(idx), Some(substitute_der(expr)))
        } else {
            (None, None)
        };
        out.push(EventBinding {
            name: ev.name.clone(),
            lhs: substitute_der(&ev.lhs),
            rhs: substitute_der(&ev.rhs),
            direction: ev.direction_code(),
            stop: ev.stops(),
            set_index,
            set_expr,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_event(target: &str) -> DynamicEvent {
        DynamicEvent {
            name: "latch".into(),
            lhs: Expr::var("v"),
            rhs: Expr::num(0.0),
            direction: Some("falling".into()),
            action: "set".into(),
            set_var: Some(target.into()),
            set_expr: Some(Expr::call("der", vec![Expr::var("h")])),
        }
    }

    /// `substituteDer` stand-in: `der(X)` → `der$X`.
    fn subst(e: &Expr) -> Expr {
        match e {
            Expr::Call { function, args } if function == "der" && args.len() == 1 => {
                match &args[0] {
                    Expr::Var(name) => Expr::var(format!("der${name}")),
                    _ => e.clone(),
                }
            }
            _ => e.clone(),
        }
    }

    #[test]
    fn direction_and_action_read_off_the_record() {
        let stop = DynamicEvent::new(
            "apogee",
            Expr::var("v"),
            Expr::num(0.0),
            Some("falling".into()),
            "stop",
        );
        assert!(stop.stops());
        assert!(!stop.is_set());
        assert_eq!(stop.direction_code(), -1);
        assert_eq!(stop.set_var, None);

        let any = DynamicEvent::new("x", Expr::var("a"), Expr::var("b"), None, "record");
        assert_eq!(any.direction_code(), 0);
        assert!(!any.stops());
    }

    #[test]
    fn binding_reifies_der_and_resolves_the_set_target() {
        let events = vec![set_event("h")];
        let bound =
            bind_events(&events, subst, |n| ["h", "v"].iter().position(|s| *s == n)).unwrap();
        assert_eq!(bound.len(), 1);
        assert_eq!(bound[0].direction, -1);
        assert!(!bound[0].stop);
        assert_eq!(bound[0].set_index, Some(0));
        assert_eq!(bound[0].set_expr, Some(Expr::var("der$h")));
        assert_eq!(bound[0].lhs, Expr::var("v"));
    }

    #[test]
    fn a_set_target_that_is_not_a_state_is_refused() {
        let events = vec![set_event("qdot")];
        let err = bind_events(&events, subst, |n| ["h", "v"].iter().position(|s| *s == n))
            .unwrap_err()
            .to_string_message();
        assert!(err.contains("set target 'qdot' is not a state"), "{err}");
        assert!(
            err.contains("only der(...) states can be reassigned"),
            "{err}"
        );
    }

    #[test]
    fn a_set_without_a_value_is_refused_rather_than_defaulted() {
        let mut ev = set_event("h");
        ev.set_expr = None;
        let err = bind_events(&[ev], subst, |n| ["h"].iter().position(|s| *s == n))
            .unwrap_err()
            .to_string_message();
        assert!(err.contains("needs a value"), "{err}");
    }

    #[test]
    fn stop_and_record_events_carry_no_set_index() {
        let events = vec![
            DynamicEvent::new("a", Expr::var("h"), Expr::num(0.0), None, "stop"),
            DynamicEvent::new("b", Expr::var("h"), Expr::num(1.0), None, "record"),
        ];
        let bound = bind_events(&events, subst, |_| None).unwrap();
        assert!(bound[0].stop);
        assert!(!bound[1].stop);
        assert!(bound.iter().all(|b| b.set_index.is_none()));
        assert!(bound.iter().all(|b| b.set_expr.is_none()));
    }
}
