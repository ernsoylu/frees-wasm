//! Expression and statement AST.
//!
//! Ported from `../frEES/backend/core/src/main/java/com/frees/backend/ast/`
//! (`Expr.java`, `Equation.java`, `Statement.java`).
//!
//! # Invariants carried over from the Java engine
//!
//! * **Identifiers are stored lowercase.** frees variable names are
//!   case-insensitive; `Expr::var()` / `Expr::call()` lowercase on construction
//!   exactly as the Java records do in their compact constructors.
//! * **Numeric literals are already SI.** The parser converts a unit-annotated
//!   literal at build time (`value * factor + offset`) and stores the SI display
//!   name in [`Num::unit`], mirroring `AstBuilder.visitNumberAtom`.
//! * **An equation is a residual.** The solver works with `lhs - rhs`.

use std::collections::BTreeSet;

/// Binary arithmetic operator.
///
/// The Java AST encodes these as a `char` in `Expr.BinOp`, using private-use
/// Unicode points for the element-wise forms (`EquationParser.ELEMENT_*`):
/// `⊙` = `.*`, `⊘` = `./`, `∖` = `.\`, `↑` = `.^`. A Rust enum is the
/// equivalent representation with the same semantics and no sentinel chars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    /// `\` — left division (`a \ b` == `b / a` for scalars, `A⁻¹B` for matrices).
    LeftDiv,
    Pow,
    /// `.*`
    ElemMul,
    /// `./`
    ElemDiv,
    /// `.\`
    ElemLeftDiv,
    /// `.^`
    ElemPow,
}

impl BinOp {
    /// True for the four element-wise operators.
    /// Mirrors `EquationParser.isElementWise`.
    pub fn is_element_wise(self) -> bool {
        matches!(
            self,
            BinOp::ElemMul | BinOp::ElemDiv | BinOp::ElemLeftDiv | BinOp::ElemPow
        )
    }

    /// The scalar operator an element-wise operator reduces to.
    /// Mirrors `EquationParser.scalarEquivalent`.
    pub fn scalar_equivalent(self) -> BinOp {
        match self {
            BinOp::ElemMul => BinOp::Mul,
            BinOp::ElemDiv => BinOp::Div,
            BinOp::ElemLeftDiv => BinOp::LeftDiv,
            BinOp::ElemPow => BinOp::Pow,
            other => other,
        }
    }

    /// Source spelling, used by diagnostics and the LaTeX/text printers.
    pub fn as_str(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::LeftDiv => "\\",
            BinOp::Pow => "^",
            BinOp::ElemMul => ".*",
            BinOp::ElemDiv => "./",
            BinOp::ElemLeftDiv => ".\\",
            BinOp::ElemPow => ".^",
        }
    }
}

/// Relational operator. Comparisons evaluate to `1.0` (true) or `0.0` (false).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CmpOp {
    Lt,
    Gt,
    Le,
    Ge,
    /// `<>`
    Ne,
    /// `=` used in a boolean position (IF / REPEAT conditions).
    Eq,
}

impl CmpOp {
    pub fn as_str(self) -> &'static str {
        match self {
            CmpOp::Lt => "<",
            CmpOp::Gt => ">",
            CmpOp::Le => "<=",
            CmpOp::Ge => ">=",
            CmpOp::Ne => "<>",
            CmpOp::Eq => "=",
        }
    }
}

/// Boolean connective. Evaluates to `1.0` / `0.0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogicOp {
    And,
    Or,
}

impl LogicOp {
    pub fn as_str(self) -> &'static str {
        match self {
            LogicOp::And => "and",
            LogicOp::Or => "or",
        }
    }
}

/// A frees expression.
///
/// Mirrors the twelve variants of the sealed Java interface `Expr`.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Numeric literal, already converted to SI. `unit` holds the SI display
    /// name when the source carried an annotation (`140 [kPa]` → `140000.0`
    /// with unit `Pa`).
    Num {
        value: f64,
        unit: Option<String>,
        is_imaginary: bool,
    },
    /// String literal: `'hello'`.
    Str(String),
    /// Variable reference. Always lowercase.
    Var(String),
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Neg(Box<Expr>),
    /// Function or intrinsic call. Name is always lowercase.
    Call {
        function: String,
        args: Vec<Expr>,
    },
    /// `a[i]`, `a[i, j]`. Name is always lowercase.
    ArrayAccess {
        name: String,
        indices: Vec<Expr>,
    },
    /// An index range `start:end` inside an array subscript.
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
    },
    /// `[1, 2; 3, 4]`
    ArrayLiteral(Vec<Expr>),
    Compare {
        op: CmpOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Logical {
        op: LogicOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Not(Box<Expr>),
}

impl Expr {
    /// Numeric literal without a unit.
    pub fn num(value: f64) -> Expr {
        Expr::Num {
            value,
            unit: None,
            is_imaginary: false,
        }
    }

    /// Variable reference; lowercases like the Java compact constructor.
    pub fn var(name: impl AsRef<str>) -> Expr {
        Expr::Var(name.as_ref().to_ascii_lowercase())
    }

    /// Call node; lowercases the function name like the Java compact constructor.
    pub fn call(function: impl AsRef<str>, args: Vec<Expr>) -> Expr {
        Expr::Call {
            function: function.as_ref().to_ascii_lowercase(),
            args,
        }
    }

    pub fn bin(op: BinOp, left: Expr, right: Expr) -> Expr {
        Expr::BinOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Every variable name appearing in this expression, sorted.
    ///
    /// Port of `Expr.variables()`. Two intrinsics bind a dummy variable that
    /// must **not** escape as a system unknown:
    ///
    /// * `sum(v, lo, hi, body)` / `product(v, lo, hi, body)` — `v` is bound over
    ///   `body` only; `lo` and `hi` are ordinary sub-expressions.
    /// * `gaussintegral(f, t, a, b[, points])` — `t` is bound over `f` only.
    pub fn variables(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        self.collect_variables(&mut out);
        out
    }

    fn collect_variables(&self, out: &mut BTreeSet<String>) {
        match self {
            Expr::Num { .. } | Expr::Str(_) => {}
            Expr::Var(name) => {
                out.insert(name.clone());
            }
            Expr::Neg(inner) | Expr::Not(inner) => inner.collect_variables(out),
            Expr::BinOp { left, right, .. }
            | Expr::Compare { left, right, .. }
            | Expr::Logical { left, right, .. }
            | Expr::Range {
                start: left,
                end: right,
            } => {
                left.collect_variables(out);
                right.collect_variables(out);
            }
            Expr::ArrayLiteral(elements) => {
                for e in elements {
                    e.collect_variables(out);
                }
            }
            Expr::ArrayAccess { name, indices } => {
                out.insert(name.clone());
                for idx in indices {
                    idx.collect_variables(out);
                }
            }
            Expr::Call { function, args } => Self::collect_call_variables(function, args, out),
        }
    }

    fn collect_call_variables(function: &str, args: &[Expr], out: &mut BTreeSet<String>) {
        // sum/product(v, lo, hi, body): `v` is bound over `body`.
        if (function == "sum" || function == "product") && args.len() == 4 {
            if let Expr::Var(bound) = &args[0] {
                args[1].collect_variables(out);
                args[2].collect_variables(out);
                let mut body = BTreeSet::new();
                args[3].collect_variables(&mut body);
                body.remove(bound);
                out.extend(body);
                return;
            }
        }
        // gaussintegral(f, t, a, b[, points]): `t` is bound over `f`.
        if function == "gaussintegral" && args.len() >= 4 {
            if let Expr::Var(bound) = &args[1] {
                let mut integrand = BTreeSet::new();
                args[0].collect_variables(&mut integrand);
                integrand.remove(bound);
                out.extend(integrand);
                for a in &args[2..] {
                    a.collect_variables(out);
                }
                return;
            }
        }
        for a in args {
            a.collect_variables(out);
        }
    }
}

/// A single equation `lhs = rhs`. The solver works with the residual `lhs - rhs`.
///
/// Port of `Equation.java`. `source_text` is retained verbatim so diagnostics
/// can quote the user's own line (the parent engine's "diagnostics are
/// source-mapped, never mangled scalars" rule).
#[derive(Debug, Clone, PartialEq)]
pub struct Equation {
    pub lhs: Expr,
    pub rhs: Expr,
    pub source_text: String,
}

impl Equation {
    pub fn new(lhs: Expr, rhs: Expr, source_text: impl Into<String>) -> Equation {
        Equation {
            lhs,
            rhs,
            source_text: source_text.into(),
        }
    }

    /// Union of the variables on both sides.
    pub fn variables(&self) -> BTreeSet<String> {
        let mut vars = self.lhs.variables();
        vars.extend(self.rhs.variables());
        vars
    }

    /// The residual expression `lhs - rhs`.
    pub fn residual(&self) -> Expr {
        Expr::bin(BinOp::Sub, self.lhs.clone(), self.rhs.clone())
    }
}

/// A top-level statement.
///
/// Port of the sealed Java interface `Statement`.
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Eq(Equation),
    /// `FOR i = start TO end … END`
    For {
        var_name: String,
        start: Expr,
        end: Expr,
        body: Vec<Statement>,
    },
    /// `SYMBOLIC s, t` — declares independent symbolic variables. Equations
    /// involving one are identities that hold for all values of it, not numeric
    /// equations to solve for it.
    Symbolic(Vec<String>),
    /// `CALL name(inputs : outputs)` — invokes a PROCEDURE or MODULE. Flattening
    /// turns this into equations that bind the outputs.
    CallProc {
        name: String,
        inputs: Vec<Expr>,
        outputs: Vec<Expr>,
        source_text: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_lowercased() {
        assert_eq!(Expr::var("Tin"), Expr::Var("tin".into()));
        match Expr::call("Enthalpy", vec![]) {
            Expr::Call { function, .. } => assert_eq!(function, "enthalpy"),
            other => panic!("expected a call, got {other:?}"),
        }
    }

    #[test]
    fn variables_are_collected_from_both_sides() {
        // x + y = z * 2
        let eq = Equation::new(
            Expr::bin(BinOp::Add, Expr::var("x"), Expr::var("Y")),
            Expr::bin(BinOp::Mul, Expr::var("z"), Expr::num(2.0)),
            "x + y = z * 2",
        );
        let vars: Vec<_> = eq.variables().into_iter().collect();
        assert_eq!(vars, vec!["x", "y", "z"]);
    }

    #[test]
    fn sum_binds_its_index_variable() {
        // sum(i, 1, n, a[i]) must expose `n` and `a` but never `i`.
        let e = Expr::call(
            "sum",
            vec![
                Expr::var("i"),
                Expr::num(1.0),
                Expr::var("n"),
                Expr::ArrayAccess {
                    name: "a".into(),
                    indices: vec![Expr::var("i")],
                },
            ],
        );
        let vars: Vec<_> = e.variables().into_iter().collect();
        assert_eq!(vars, vec!["a", "n"]);
    }

    #[test]
    fn gaussintegral_binds_its_integration_variable() {
        // gaussintegral(t * k, t, 0, 1) exposes `k`, not `t`.
        let e = Expr::call(
            "gaussintegral",
            vec![
                Expr::bin(BinOp::Mul, Expr::var("t"), Expr::var("k")),
                Expr::var("t"),
                Expr::num(0.0),
                Expr::num(1.0),
            ],
        );
        let vars: Vec<_> = e.variables().into_iter().collect();
        assert_eq!(vars, vec!["k"]);
    }

    #[test]
    fn array_access_exposes_the_array_itself() {
        let e = Expr::ArrayAccess {
            name: "speed".into(),
            indices: vec![Expr::var("n")],
        };
        let vars: Vec<_> = e.variables().into_iter().collect();
        assert_eq!(vars, vec!["n", "speed"]);
    }

    #[test]
    fn element_wise_operators_reduce_to_scalar_equivalents() {
        assert!(BinOp::ElemMul.is_element_wise());
        assert!(!BinOp::Mul.is_element_wise());
        assert_eq!(BinOp::ElemDiv.scalar_equivalent(), BinOp::Div);
        assert_eq!(BinOp::ElemLeftDiv.scalar_equivalent(), BinOp::LeftDiv);
        assert_eq!(BinOp::Add.scalar_equivalent(), BinOp::Add);
    }
}
