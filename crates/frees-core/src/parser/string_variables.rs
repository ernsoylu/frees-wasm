//! Resolution of string variables — identifiers with a trailing `$` (by
//! long-standing convention), e.g. `R$ = 'R134a'`.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/parser/
//! StringVariables.java`, run at the Java position: the last step of
//! `EquationParser.parseResult`, after component expansion, CALL flattening
//! and matrix expansion have produced the flat equation list.
//!
//! A string variable is defined by an equation binding it to a string literal
//! (on either side). Definitions are compile-time constants: every use of the
//! variable is replaced by its literal value — including fluid names inside
//! synthetic `prop$` property calls — and the definition equations are removed
//! from the numeric system, so they do not count towards the degrees of
//! freedom.

use std::collections::BTreeMap;
use std::collections::HashMap;

use crate::ast::{Equation, Expr};
use crate::diag::{FreesError, Result};

/// `StringVariables.resolve`: lift `IDENT$ = 'literal'` definitions out of the
/// equation list and substitute the literal at every use.
pub fn resolve(
    equations: Vec<Equation>,
    display_names: &BTreeMap<String, String>,
) -> Result<Vec<Equation>> {
    let mut bindings: HashMap<String, String> = HashMap::new();
    let mut remaining = Vec::with_capacity(equations.len());

    for eq in equations {
        let binding = match (&eq.lhs, &eq.rhs) {
            (Expr::Var(name), Expr::Str(value)) if name.ends_with('$') => {
                Some((name.clone(), value.clone()))
            }
            (Expr::Str(value), Expr::Var(name)) if name.ends_with('$') => {
                Some((name.clone(), value.clone()))
            }
            _ => None,
        };
        match binding {
            Some((var, value)) => {
                if let Some(previous) = bindings.insert(var.clone(), value.clone()) {
                    if previous != value {
                        return Err(FreesError::parse(format!(
                            "String variable '{}' is defined twice with different values \
                             ('{previous}' and '{value}').",
                            display(&var, display_names)
                        )));
                    }
                }
            }
            None => remaining.push(eq),
        }
    }

    // Unconditional, as in the Java: the walk itself is what rejects an
    // unbound `$` use, so it must run even when no definition exists at all.
    remaining
        .into_iter()
        .map(|eq| {
            Ok(Equation {
                lhs: substitute(&eq.lhs, &bindings, display_names)?,
                rhs: substitute(&eq.rhs, &bindings, display_names)?,
                source_text: eq.source_text,
            })
        })
        .collect()
}

fn display<'a>(name: &'a str, display_names: &'a BTreeMap<String, String>) -> &'a str {
    display_names.get(name).map(String::as_str).unwrap_or(name)
}

fn substitute(
    e: &Expr,
    bindings: &HashMap<String, String>,
    display_names: &BTreeMap<String, String>,
) -> Result<Expr> {
    let sub = |inner: &Expr| substitute(inner, bindings, display_names);
    Ok(match e {
        Expr::Num { .. } | Expr::Str(_) => e.clone(),
        Expr::Var(name) => {
            if !name.ends_with('$') {
                return Ok(e.clone());
            }
            match bindings.get(name) {
                Some(value) => Expr::Str(value.clone()),
                None => {
                    let shown = display(name, display_names);
                    return Err(FreesError::parse(format!(
                        "String variable '{shown}' is not defined. \
                         Assign it with {shown} = '...'."
                    )));
                }
            }
        }
        Expr::Neg(operand) => Expr::Neg(Box::new(sub(operand)?)),
        Expr::BinOp { op, left, right } => Expr::BinOp {
            op: *op,
            left: Box::new(sub(left)?),
            right: Box::new(sub(right)?),
        },
        Expr::Call { function, args } => Expr::Call {
            function: resolve_function(function, bindings, display_names)?,
            args: args.iter().map(sub).collect::<Result<Vec<_>>>()?,
        },
        Expr::ArrayAccess { name, indices } => Expr::ArrayAccess {
            name: name.clone(),
            indices: indices.iter().map(sub).collect::<Result<Vec<_>>>()?,
        },
        Expr::Range { start, end } => Expr::Range {
            start: Box::new(sub(start)?),
            end: Box::new(sub(end)?),
        },
        Expr::ArrayLiteral(elements) => {
            Expr::ArrayLiteral(elements.iter().map(sub).collect::<Result<Vec<_>>>()?)
        }
        Expr::Compare { op, left, right } => Expr::Compare {
            op: *op,
            left: Box::new(sub(left)?),
            right: Box::new(sub(right)?),
        },
        Expr::Logical { op, left, right } => Expr::Logical {
            op: *op,
            left: Box::new(sub(left)?),
            right: Box::new(sub(right)?),
        },
        Expr::Not(operand) => Expr::Not(Box::new(sub(operand)?)),
    })
}

/// Resolves a string-variable fluid inside a synthetic property call name.
/// The parser encodes `Enthalpy(R$, T=..., x=1)` as `prop$enthalpy$r$$t$x`;
/// the fluid's trailing `$` produces an empty segment after it when split on
/// `$`.
fn resolve_function(
    function: &str,
    bindings: &HashMap<String, String>,
    display_names: &BTreeMap<String, String>,
) -> Result<String> {
    if !function.starts_with("prop$") {
        return Ok(function.to_string());
    }
    let parts: Vec<&str> = function.split('$').collect();
    // parts: ["prop", output, fluid, ("" if fluid was a string var), indicators…]
    if parts.len() < 4 || !parts[3].is_empty() {
        return Ok(function.to_string());
    }
    let fluid_var = format!("{}$", parts[2]);
    let Some(value) = bindings.get(&fluid_var) else {
        let shown = display(&fluid_var, display_names);
        return Err(FreesError::parse(format!(
            "String variable '{shown}' used as a fluid name is not defined. \
             Assign it with {shown} = '...'."
        )));
    };
    if !is_valid_fluid_value(value) {
        return Err(FreesError::parse(format!(
            "'{value}' (value of {}) is not a valid fluid name.",
            display(&fluid_var, display_names)
        )));
    }
    let mut rebuilt = format!("prop${}${}", parts[1], value.to_lowercase());
    for part in &parts[4..] {
        rebuilt.push('$');
        rebuilt.push_str(part);
    }
    Ok(rebuilt)
}

/// The Java `value.matches("[A-Za-z]\\w*")` fluid-value check.
fn is_valid_fluid_value(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    //! The string-variable half of the Java `StringLiteralTest` (the literal
    //! half lives with the evaluator tests in `eval.rs`).

    use super::*;
    use crate::eval::{eval, Scope};
    use crate::parser::expand::expand_document;
    use crate::parser::parse_document;

    fn resolved(source: &str) -> Result<Vec<Equation>> {
        let doc = parse_document(source)?;
        resolve(expand_document(&doc)?, &doc.display_names)
    }

    fn value_of(eq: &Equation) -> f64 {
        eval(&eq.rhs, &Scope::new()).expect("rhs evaluates")
    }

    /// `stringVariableResolvesAndLeavesNumericSystem`
    #[test]
    fn string_variable_resolves_and_leaves_numeric_system() {
        let equations = resolved("d$ = '1010'\nx = BaseConvert(d$, 2, 10)").unwrap();
        assert_eq!(equations.len(), 1);
        assert_eq!(equations[0].lhs, Expr::Var("x".into()));
        assert!((value_of(&equations[0]) - 10.0).abs() < 1e-9);
    }

    /// `stringVariableDefinitionReversedSidesWorks`
    #[test]
    fn string_variable_definition_reversed_sides_works() {
        let equations = resolved("'FF' = d$\nx = BaseConvert(d$, 16, 10)").unwrap();
        assert_eq!(equations.len(), 1);
        assert!((value_of(&equations[0]) - 255.0).abs() < 1e-9);
    }

    /// `stringVariableAsFluidName`
    #[test]
    fn string_variable_as_fluid_name() {
        let equations = resolved("R$ = 'R134a'\nh = Enthalpy(R$, T=300, x=1)").unwrap();
        assert_eq!(equations.len(), 1);
        let Expr::Call { function, .. } = &equations[0].rhs else {
            panic!("expected a call, got {:?}", equations[0].rhs);
        };
        assert!(function.starts_with("prop$enthalpy$r134a$"), "{function}");
    }

    /// `undefinedStringVariableThrows` — no definitions exist at all, and the
    /// use must still be rejected at parse time, not at evaluation.
    #[test]
    fn undefined_string_variable_throws() {
        let err = resolved("x = BaseConvert(d$, 2, 10)").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("d$"), "{message}");
        assert!(message.contains("not defined"), "{message}");
    }

    /// `undefinedStringVariableThrows`, fluid-name arm: the `prop$…$d$$…`
    /// encoding forces the rewrite regardless of other bindings.
    #[test]
    fn undefined_string_fluid_throws() {
        let err = resolved("h = Enthalpy(d$, T=300, x=1)").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("d$"), "{message}");
        assert!(message.contains("not defined"), "{message}");
    }

    /// `conflictingStringVariableDefinitionsThrow`
    #[test]
    fn conflicting_string_variable_definitions_throw() {
        let err = resolved("a$ = 'one'\na$ = 'two'").unwrap_err();
        assert!(err.to_string().contains("defined twice"), "{err}");
    }

    /// A repeated definition with the *same* value is not a conflict.
    #[test]
    fn repeated_identical_definition_is_allowed() {
        let equations = resolved("a$ = 'one'\na$ = 'one'").unwrap();
        assert!(equations.is_empty());
    }

    /// `stringVariableIsCaseInsensitive` — the parser lowercases identifiers,
    /// so `D$` and `d$` are the same binding.
    #[test]
    fn string_variable_is_case_insensitive() {
        let equations = resolved("D$ = 'FF'\nx = BaseConvert(d$, 16, 10)").unwrap();
        assert_eq!(equations.len(), 1);
        assert!((value_of(&equations[0]) - 255.0).abs() < 1e-9);
    }

    /// The definition must not count towards the degrees of freedom: the
    /// golden for `heisler-transient` has `geom$` in `display_names` but not
    /// in `variables`.
    #[test]
    fn definition_leaves_no_numeric_variable() {
        let equations = resolved("geom$ = 'wall'\ny = 2").unwrap();
        assert_eq!(equations.len(), 1);
        assert_eq!(equations[0].lhs, Expr::Var("y".into()));
    }
}
