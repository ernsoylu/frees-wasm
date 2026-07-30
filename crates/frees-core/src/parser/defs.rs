//! Definition AST: `FUNCTION` / `PROCEDURE` / `MODULE` / `TABLE` blocks.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/ast/ProcDef.java`
//! and `ProcStatement.java`. These types are a **fixed contract** for the
//! Phase-4 implementation agents: the block parser fills them, the procedure
//! evaluator executes them, and the expression evaluator dispatches into them
//! through [`crate::eval::EvalContext`].
//!
//! A note on multi-output functions: the grammar allows
//! `FUNCTION [a, b] = f(x) … END`, but the Java `FunctionDef` record carries no
//! outputs list — `EquationParser`/`AstBuilder` desugar the multi-output form.
//! The port must mirror whatever the Java does (verify at the call site), not
//! grow an extra field here.

use crate::ast::{Equation, Expr, Statement};

/// A statement inside a `FUNCTION` or `PROCEDURE` body. Unlike top-level
/// [`Statement`]s, these execute **sequentially**, not as equations.
///
/// Port of `ProcStatement.java`.
#[derive(Debug, Clone, PartialEq)]
pub enum ProcStatement {
    /// Sequential assignment: `var := expr`.
    Assign { var_name: String, value: Expr },
    /// `IF condition THEN … [ELSE …] END`.
    IfElse {
        condition: Expr,
        then_branch: Vec<ProcStatement>,
        else_branch: Vec<ProcStatement>,
    },
    /// `REPEAT … UNTIL condition`.
    RepeatUntil {
        body: Vec<ProcStatement>,
        condition: Expr,
    },
    /// An equation used inside a body for intermediate relations.
    Eq(Equation),
    /// `FOR var = start TO end … END`.
    For {
        var_name: String,
        start: Expr,
        end: Expr,
        body: Vec<ProcStatement>,
    },
    /// `WHILE condition DO … END`.
    While {
        condition: Expr,
        body: Vec<ProcStatement>,
    },
}

/// `FUNCTION name(params) … END` — returns a single value assigned to the
/// function name via `:=`. Port of `ProcDef.FunctionDef`.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef {
    /// Lowercase canonical name.
    pub name: String,
    /// Parameter names, lowercase, declaration order.
    pub params: Vec<String>,
    pub body: Vec<ProcStatement>,
    /// Declared output unit (`FUNCTION f(x) [m/s]`), `None` when absent.
    pub output_unit: Option<String>,
    /// Declared per-parameter units, aligned with `params`; `None` when the
    /// declaration carried none at all.
    pub param_units: Option<Vec<Option<String>>>,
}

/// `PROCEDURE name(inputs : outputs) … END` — outputs are assigned via `:=`
/// and injected as equations into the solver. Port of `ProcDef.ProcedureDef`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureDef {
    pub name: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub body: Vec<ProcStatement>,
}

/// `MODULE name(inputs : outputs) … END` — body contains `=` equations that
/// are grafted into the main system with namespaced variable names. Port of
/// `ProcDef.ModuleDef`.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDef {
    pub name: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub body: Vec<Statement>,
}

/// One tabulated curve: its family-parameter value (`None` for a lone curve)
/// and sample arrays sorted ascending by x. Port of `ProcDef.Curve`.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    pub param: Option<f64>,
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
}

/// `TABLE name(arg [: param = v1, v2, …]) [flags] … END` — a tabulated
/// function: one curve evaluates `name(x)`, a family evaluates
/// `name(x, param)` by interpolating across curves (`CurveInterpolator`).
/// Port of `ProcDef.FunctionTableDef`.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionTableDef {
    pub name: String,
    /// Argument names — first is the lookup argument, a second names the
    /// family parameter.
    pub arg_names: Vec<String>,
    pub x_log: bool,
    pub y_log: bool,
    pub curves: Vec<Curve>,
    /// Declared output unit, `None` when absent.
    pub output_unit: Option<String>,
    /// Declared argument units, aligned with `arg_names`.
    pub arg_units: Option<Vec<Option<String>>>,
}

/// Every definition a document declared, by kind. Names are lowercase; a
/// later definition of the same name replaces an earlier one (verify the Java
/// rule at the parser site and document it there).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Definitions {
    pub functions: Vec<FunctionDef>,
    pub procedures: Vec<ProcedureDef>,
    pub modules: Vec<ModuleDef>,
    pub tables: Vec<FunctionTableDef>,
}

impl Definitions {
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
            && self.procedures.is_empty()
            && self.modules.is_empty()
            && self.tables.is_empty()
    }

    pub fn function(&self, lowercase_name: &str) -> Option<&FunctionDef> {
        self.functions.iter().find(|f| f.name == lowercase_name)
    }

    pub fn procedure(&self, lowercase_name: &str) -> Option<&ProcedureDef> {
        self.procedures.iter().find(|p| p.name == lowercase_name)
    }

    pub fn module(&self, lowercase_name: &str) -> Option<&ModuleDef> {
        self.modules.iter().find(|m| m.name == lowercase_name)
    }

    pub fn table(&self, lowercase_name: &str) -> Option<&FunctionTableDef> {
        self.tables.iter().find(|t| t.name == lowercase_name)
    }
}
