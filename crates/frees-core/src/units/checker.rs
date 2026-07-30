//! Dimensional consistency checking and SI unit inference.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/units/UnitChecker.java`
//! (798 lines). Traverses each equation's AST verifying dimensional homogeneity
//! across `=` and `+`/`-`, and derives SI units for variables defined by
//! equations whose other side has known dimensions (`P = m*g/A` gets `Pa` once
//! `m`, `g`, `A` are known).
//!
//! Semantics carried over from the parent engine:
//!
//! * **Unit problems are warnings, never errors** — nothing here blocks a
//!   solve. This module only reports.
//! * Variables with blank units are wildcards (unknown); the explicit
//!   dimensionless marker is `-`.
//! * Numeric literals were already converted to SI at parse time and carry
//!   their SI display name in [`Expr::Num`]'s `unit` field.
//! * Derivation iterates to a fixpoint (max 8 passes, exactly as the Java) so
//!   inference chains propagate regardless of equation order.
//!
//! Deliberate, recorded divergences from the Java:
//!
//! * Intrinsic calls with **no arguments** degrade to an agnostic (or, in the
//!   default branch, dimensionless) result where the Java would throw
//!   `IndexOutOfBoundsException`.
//! * Unknown-unit message text: the Rust [`UnitRegistry`] reports a malformed
//!   expression as a whole (`FreesError::UnknownUnit` carries the offending
//!   name or the full expression), so `Cannot parse unit:` messages quote the
//!   whole expression where the Java quotes the single failing token. The
//!   `Unknown unit: '<name>' in '<expression>'` shape is identical.

use std::collections::{BTreeMap, HashMap};

use crate::ast::{BinOp, Equation, Expr};
use crate::diag::FreesError;
use crate::units::quantity::{Dims, Quantity, DIMENSIONS};
use crate::units::UnitRegistry;

/// Warnings plus SI units derived for computed variables.
///
/// Port of the Java `UnitChecker.Result` record. `warnings` feed the
/// `unitWarnings[]` list and `inferred` the `inferredUnits{}` map of
/// `CheckResponse`; the strings are displayed verbatim by the frontend.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitReport {
    /// Human-readable warning sentences, in Java emission order.
    pub warnings: Vec<String>,
    /// Lowercase variable name → SI display unit (`"Pa"` style, `"-"` for
    /// dimensionless). Contains only variables *derived* here, never the
    /// declared ones.
    pub inferred: BTreeMap<String, String>,
}

/// Check all equations against declared variable units and derive SI units for
/// computed variables.
///
/// `known_units` maps variable names (any case) to unit expressions already
/// declared — via literals or `VariableInfo`. Blank values are wildcards.
///
/// Port of the Java two-argument `UnitChecker.check`.
pub fn check_units(equations: &[Equation], known_units: &BTreeMap<String, String>) -> UnitReport {
    check_units_full(equations, known_units, &BTreeMap::new(), &BTreeMap::new())
}

/// As [`check_units`], plus declared units for TABLE/FUNCTION calls: output
/// units (function name → SI unit) carry the call's result dimensions, and
/// argument units (function name → per-argument SI units, `None` = undeclared)
/// ground the call's argument variables. Together they let variables computed
/// from lookups/functions resolve instead of collapsing to dimensionless.
///
/// Port of the Java four-argument `UnitChecker.check`.
pub fn check_units_full(
    equations: &[Equation],
    variable_units: &BTreeMap<String, String>,
    function_units: &BTreeMap<String, String>,
    function_input_units: &BTreeMap<String, Vec<Option<String>>>,
) -> UnitReport {
    let mut warnings: Vec<String> = Vec::new();
    let mut dims: HashMap<String, Quantity> = HashMap::new();

    for (name, units) in variable_units {
        if units.trim().is_empty() {
            continue;
        }
        match UnitRegistry::parse(units) {
            Ok(q) => {
                dims.insert(name.to_lowercase(), q);
            }
            Err(e) => warnings.push(format!("Variable {name}: {}", unit_error_text(&e, units))),
        }
    }

    let mut function_dims: HashMap<String, Quantity> = HashMap::new();
    for (name, units) in function_units {
        if units.trim().is_empty() {
            continue;
        }
        // A bad declared unit just leaves the function unitless.
        if let Ok(q) = UnitRegistry::parse(units) {
            function_dims.insert(name.to_lowercase(), q);
        }
    }

    // Argument-unit declarations become synthetic "argExpr = X[unit]" equations,
    // so the existing rearrangement grounds the argument's variable (e.g.
    // fanCurve(Vair/f_rpm) with arg unit m^3/s grounds Vair).
    let mut synthetic: Vec<Equation> = Vec::new();
    if !function_input_units.is_empty() {
        let input_units: BTreeMap<String, &Vec<Option<String>>> = function_input_units
            .iter()
            .map(|(k, v)| (k.to_lowercase(), v))
            .collect();
        for eq in equations {
            collect_arg_unit_equations(&eq.lhs, &input_units, &mut synthetic);
            collect_arg_unit_equations(&eq.rhs, &input_units, &mut synthetic);
        }
    }

    // Derivation passes (warnings suppressed): when an equation has exactly one
    // variable with unknown dimensions and it participates multiplicatively,
    // its dimensions are solved by rearrangement (F = m*g with F and g known
    // gives m = kg). Iterates to a fixpoint so chains propagate.
    let mut checker = Checker {
        variable_dims: dims,
        function_dims,
        warnings: Vec::new(),
        collect_warnings: false,
        current_equation: String::new(),
    };
    let mut derived: BTreeMap<String, String> = BTreeMap::new();
    for _pass in 0..8 {
        let mut changed = false;
        for eq in equations.iter().chain(synthetic.iter()) {
            let unknowns: Vec<String> = eq
                .variables()
                .into_iter()
                .filter(|v| !checker.variable_dims.contains_key(v))
                .collect();
            if unknowns.is_empty() {
                continue;
            }
            checker.current_equation.clone_from(&eq.source_text);
            if unknowns.len() == 1 {
                let unknown = &unknowns[0];
                if let Some(solved) = checker.solve_dims_for(eq, unknown) {
                    derived.insert(unknown.clone(), UnitRegistry::si_name(&solved.dims));
                    checker.variable_dims.insert(unknown.clone(), solved);
                    changed = true;
                    continue;
                }
            }
            // Additive homogeneity: in T[3] - T[4], the unknown T[3] must carry
            // T[4]'s dimensions, no matter how many other unknowns the equation
            // has. This grounds variables that appear only implicitly (on both
            // sides of their equations).
            for unknown in &unknowns {
                if checker.variable_dims.contains_key(unknown) {
                    continue;
                }
                if let Some(additive) = checker.additive_dims_of_equation(eq, unknown) {
                    derived.insert(unknown.clone(), UnitRegistry::si_name(&additive.dims));
                    checker.variable_dims.insert(unknown.clone(), additive);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    // Check pass: warnings on.
    checker.collect_warnings = true;
    for eq in equations {
        checker.current_equation.clone_from(&eq.source_text);
        let lhs = checker.dim_of(&eq.lhs);
        let rhs = checker.dim_of(&eq.rhs);
        if lhs.known && rhs.known && !lhs.quantity.same_dimensions_as(&rhs.quantity) {
            checker.warnings.push(format!(
                "{}: the units of the left side [{}] do not match the right side [{}].",
                eq.source_text,
                lhs.quantity.dimension_string(),
                rhs.quantity.dimension_string()
            ));
        }
    }

    warnings.extend(checker.warnings);
    UnitReport {
        warnings,
        inferred: derived,
    }
}

/// SI unit expression of each property function output, or `None` when the
/// output is not recognised. Port of the Java `UnitChecker.propertyUnit`.
pub fn property_unit(output: &str) -> Option<&'static str> {
    Some(match output {
        "temperature" | "wetbulb" | "dewpoint" | "t_crit" => "K",
        "pressure" | "p_crit" => "Pa",
        "enthalpy" | "intenergy" | "gibbs" => "J/kg",
        "entropy" | "cp" | "cv" | "specheat" => "J/kg-K",
        "density" => "kg/m^3",
        "volume" => "m^3/kg",
        "viscosity" => "Pa-s",
        "conductivity" => "W/m-K",
        "soundspeed" => "m/s",
        "molarmass" => "kg/mol",
        "heatingvalue" => "J/kg",
        "quality"
        | "relhum"
        | "humrat"
        | "stoichafr"
        | "compressibility"
        | "compressibilityfactor" => "-",
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// A dimension that may be unknown (wildcard from a variable with blank units).
/// Port of the Java `UnitChecker.Dim` record.
#[derive(Debug, Clone, Copy)]
struct Dim {
    quantity: Quantity,
    known: bool,
}

impl Dim {
    fn unknown() -> Dim {
        Dim {
            quantity: Quantity::dimensionless(1.0),
            known: false,
        }
    }

    fn of(quantity: Quantity) -> Dim {
        Dim {
            quantity,
            known: true,
        }
    }
}

/// Known-dimension contribution plus the net exponent of the unknown.
/// Port of the Java `UnitChecker.DimTerm`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct DimTerm {
    dims: Dims,
    unknown_exponent: f64,
}

/// Reproduce the Java `UnitRegistry.UnknownUnitException` message shape from
/// the Rust error. `FreesError::UnknownUnit` carries a bare unit *name* when a
/// lookup failed and the whole expression when the syntax was malformed; a
/// pure-alphabetic payload can only be a name (any malformed expression
/// necessarily contains a non-letter).
fn unit_error_text(err: &FreesError, full: &str) -> String {
    match err {
        FreesError::UnknownUnit { unit }
            if !unit.is_empty() && unit.chars().all(char::is_alphabetic) =>
        {
            format!("Unknown unit: '{unit}' in '{full}'")
        }
        FreesError::UnknownUnit { unit } => format!("Cannot parse unit: '{unit}' in '{full}'"),
        other => other.to_string(),
    }
}

/// True when `name` appears as a variable of `e`. Port of `UnitChecker.mentions`.
fn mentions(e: &Expr, name: &str) -> bool {
    e.variables().contains(name)
}

/// Walks an expression and, for each call to a function with declared argument
/// units, emits a synthetic `argExpr = X[unit]` equation that the derivation
/// passes solve to ground the argument's variable.
/// Port of `UnitChecker.collectArgUnitEquations`.
fn collect_arg_unit_equations(
    e: &Expr,
    input_units: &BTreeMap<String, &Vec<Option<String>>>,
    out: &mut Vec<Equation>,
) {
    match e {
        Expr::Call { function, args } => {
            if let Some(arg_units) = input_units.get(function) {
                for (arg, unit) in args.iter().zip(arg_units.iter()) {
                    if let Some(unit) = unit {
                        out.push(Equation::new(
                            arg.clone(),
                            Expr::Num {
                                value: 0.0,
                                unit: Some(unit.clone()),
                                is_imaginary: false,
                            },
                            "<arg unit>",
                        ));
                    }
                }
            }
            for arg in args {
                collect_arg_unit_equations(arg, input_units, out);
            }
        }
        Expr::BinOp { left, right, .. } => {
            collect_arg_unit_equations(left, input_units, out);
            collect_arg_unit_equations(right, input_units, out);
        }
        Expr::Neg(operand) => collect_arg_unit_equations(operand, input_units, out),
        Expr::ArrayLiteral(elements) => {
            for el in elements {
                collect_arg_unit_equations(el, input_units, out);
            }
        }
        // Leaves carry no calls (and the Java walks no other variants).
        _ => {}
    }
}

/// Dense linear-algebra, signal-processing and regression intrinsics whose
/// results' dimensions are not tracked: stay agnostic rather than asserting
/// (and warning about) dimensionless arguments.
const AGNOSTIC_CALL_PREFIXES: &[&str] = &[
    "det$", "qr$", "chol$", "expm$", "svd$", "fft$", "ifft$", "conv$", "linfit$", "polyfit$",
    "interp2$",
];

/// Control-systems intrinsics whose results are treated as dimensionless.
const DIMENSIONLESS_CALL_PREFIXES: &[&str] = &[
    "ss2tf$",
    "tf2ss$",
    "ctrb$",
    "obsv$",
    "rank$",
    "ss2ss$",
    "ss_series$",
    "ss_parallel$",
    "ss_feedback$",
    "stepinfo$",
    "pade$",
    "rlocus$",
    "zp2tf$",
    "tf2zp$",
    "series$",
    "parallel$",
    "feedback$",
    "pole$",
    "zero$",
    "bode$",
    "nyquist$",
    "margin$",
    "step$",
    "impulse$",
    "lsim$",
    "lqr$",
    "place$",
    "pidtune$",
    "routh$",
    "c2d$",
    "d2c$",
    "residue$",
    "nichols$",
    "errorconst$",
    "mason$",
];

/// The traversal state: declared/derived variable dimensions, declared
/// function output dimensions, and the warning sink.
struct Checker {
    variable_dims: HashMap<String, Quantity>,
    function_dims: HashMap<String, Quantity>,
    warnings: Vec<String>,
    collect_warnings: bool,
    current_equation: String,
}

impl Checker {
    fn warn(&mut self, message: String) {
        if self.collect_warnings {
            self.warnings.push(message);
        }
    }

    // -- derivation (rearrangement) -----------------------------------------

    /// Solves an equation dimensionally for the single unknown variable:
    /// analyzes both sides as (known dims) * unknown^e and rearranges. Returns
    /// `None` when the unknown does not appear purely multiplicatively or the
    /// other dimensions cannot be determined.
    fn solve_dims_for(&mut self, eq: &Equation, unknown: &str) -> Option<Quantity> {
        let lhs = self.analyze(&eq.lhs, unknown)?;
        let rhs = self.analyze(&eq.rhs, unknown)?;
        let exponent = lhs.unknown_exponent - rhs.unknown_exponent;
        if exponent.abs() < 1e-9 {
            return None;
        }
        let mut solved = [0.0; DIMENSIONS];
        for ((s, r), l) in solved.iter_mut().zip(rhs.dims.iter()).zip(lhs.dims.iter()) {
            *s = (r - l) / exponent;
        }
        Some(Quantity::new(1.0, solved))
    }

    fn analyze(&mut self, e: &Expr, unknown: &str) -> Option<DimTerm> {
        match e {
            Expr::Num { unit, .. } => match unit {
                // A bare constant is dimensionless inside a product chain.
                None => Some(DimTerm {
                    dims: [0.0; DIMENSIONS],
                    unknown_exponent: 0.0,
                }),
                Some(u) => match UnitRegistry::parse(u) {
                    Ok(q) => Some(DimTerm {
                        dims: q.dims,
                        unknown_exponent: 0.0,
                    }),
                    Err(_) => None,
                },
            },
            Expr::Var(name) => {
                if name == unknown {
                    return Some(DimTerm {
                        dims: [0.0; DIMENSIONS],
                        unknown_exponent: 1.0,
                    });
                }
                self.variable_dims.get(name).map(|q| DimTerm {
                    dims: q.dims,
                    unknown_exponent: 0.0,
                })
            }
            Expr::Neg(operand) => self.analyze(operand, unknown),
            Expr::BinOp { op, left, right } => match op {
                BinOp::Mul => {
                    let a = self.analyze(left, unknown);
                    let b = self.analyze(right, unknown);
                    combine(a, b, 1.0)
                }
                BinOp::Div => {
                    let a = self.analyze(left, unknown);
                    let b = self.analyze(right, unknown);
                    combine(a, b, -1.0)
                }
                BinOp::Pow => {
                    if let Expr::Num { value, .. } = right.as_ref() {
                        let base = self.analyze(left, unknown)?;
                        let mut scaled = base.dims;
                        for d in scaled.iter_mut() {
                            *d *= *value;
                        }
                        Some(DimTerm {
                            dims: scaled,
                            unknown_exponent: base.unknown_exponent * value,
                        })
                    } else {
                        // Non-constant exponent: well-defined only when the
                        // unknown-free base is dimensionless, as in
                        // T * (P2/P1)^((k-1)/k), which keeps T's units.
                        if !mentions(left, unknown) && !mentions(right, unknown) {
                            let base = self.dim_of(left);
                            if base.known && base.quantity.is_dimensionless() {
                                return Some(DimTerm {
                                    dims: [0.0; DIMENSIONS],
                                    unknown_exponent: 0.0,
                                });
                            }
                        }
                        None
                    }
                }
                // The unknown inside a sum or difference cannot be isolated
                // multiplicatively; an unknown-free sub-expression falls back
                // to dimOf.
                _ => self.analyze_fallback(e, unknown),
            },
            // Function arguments containing the unknown are not invertible
            // here; unknown-free expressions fall back to dimOf.
            _ => self.analyze_fallback(e, unknown),
        }
    }

    fn analyze_fallback(&mut self, e: &Expr, unknown: &str) -> Option<DimTerm> {
        if mentions(e, unknown) {
            return None;
        }
        let dim = self.dim_of(e);
        if dim.known {
            Some(DimTerm {
                dims: dim.quantity.dims,
                unknown_exponent: 0.0,
            })
        } else {
            None
        }
    }

    // -- derivation (additive homogeneity) ----------------------------------

    /// Dimensions of the unknown forced by a sum/difference with a
    /// known-dimension partner anywhere in the equation, or `None`.
    fn additive_dims_of_equation(&mut self, eq: &Equation, unknown: &str) -> Option<Quantity> {
        self.additive_dims(&eq.lhs, unknown)
            .or_else(|| self.additive_dims(&eq.rhs, unknown))
    }

    fn additive_dims(&mut self, e: &Expr, unknown: &str) -> Option<Quantity> {
        match e {
            Expr::BinOp { op, left, right } => {
                if matches!(op, BinOp::Add | BinOp::Sub) {
                    let q = self
                        .additive_partner_dims(left, right, unknown)
                        .or_else(|| self.additive_partner_dims(right, left, unknown));
                    if q.is_some() {
                        return q;
                    }
                }
                self.additive_dims(left, unknown)
                    .or_else(|| self.additive_dims(right, unknown))
            }
            Expr::Neg(operand) => self.additive_dims(operand, unknown),
            Expr::Call { args, .. } => {
                for arg in args {
                    if let Some(q) = self.additive_dims(arg, unknown) {
                        return Some(q);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn additive_partner_dims(
        &mut self,
        candidate: &Expr,
        partner: &Expr,
        unknown: &str,
    ) -> Option<Quantity> {
        if let Expr::Var(name) = candidate {
            if name == unknown && !mentions(partner, unknown) {
                let dim = self.dim_of(partner);
                if dim.known {
                    return Some(dim.quantity);
                }
            }
        }
        None
    }

    // -- dimension propagation ----------------------------------------------

    fn dim_of(&mut self, e: &Expr) -> Dim {
        match e {
            Expr::Num { unit, .. } => self.dim_of_num(unit.as_deref()),
            Expr::Str(_) => Dim::unknown(),
            Expr::Var(name) => match self.variable_dims.get(name) {
                Some(q) => Dim::of(*q),
                None => Dim::unknown(),
            },
            Expr::Neg(operand) => self.dim_of(operand),
            Expr::BinOp { op, left, right } => self.dim_of_bin_op(*op, left, right),
            Expr::Call { function, args } => self.dim_of_call(function, args),
            Expr::ArrayAccess { .. } | Expr::Range { .. } | Expr::ArrayLiteral(_) => Dim::unknown(),
            // Comparisons and logical operators evaluate to dimensionless 1.0/0.0.
            Expr::Compare { .. } | Expr::Logical { .. } | Expr::Not(_) => {
                Dim::of(Quantity::dimensionless(1.0))
            }
        }
    }

    fn dim_of_num(&mut self, unit: Option<&str>) -> Dim {
        let Some(unit) = unit else {
            // A bare numeric constant adapts to its context.
            return Dim::unknown();
        };
        match UnitRegistry::parse(unit) {
            Ok(q) => Dim::of(q),
            Err(e) => {
                let message = format!("{}: {}", self.current_equation, unit_error_text(&e, unit));
                self.warn(message);
                Dim::unknown()
            }
        }
    }

    fn dim_of_bin_op(&mut self, op: BinOp, left_e: &Expr, right_e: &Expr) -> Dim {
        let left = self.dim_of(left_e);
        let right = self.dim_of(right_e);
        match op {
            BinOp::Add | BinOp::Sub => {
                if left.known && right.known && !left.quantity.same_dimensions_as(&right.quantity) {
                    let message = format!(
                        "{}: cannot add/subtract [{}] and [{}].",
                        self.current_equation,
                        left.quantity.dimension_string(),
                        right.quantity.dimension_string()
                    );
                    self.warn(message);
                    return left;
                }
                if left.known {
                    left
                } else {
                    right
                }
            }
            // In a product/quotient a bare numeric literal is a dimensionless
            // scale factor, so 0.5*rho keeps rho's dimensions rather than
            // collapsing to a wildcard.
            BinOp::Mul => {
                let l = as_factor(left_e, left);
                let r = as_factor(right_e, right);
                if l.known && r.known {
                    Dim::of(l.quantity.multiply(&r.quantity))
                } else {
                    Dim::unknown()
                }
            }
            BinOp::Div => {
                let l = as_factor(left_e, left);
                let r = as_factor(right_e, right);
                if l.known && r.known {
                    Dim::of(l.quantity.divide(&r.quantity))
                } else {
                    Dim::unknown()
                }
            }
            BinOp::Pow => self.dim_of_power(right_e, left),
            // Left division and the element-wise operators fall into the Java
            // switch's default: agnostic.
            _ => Dim::unknown(),
        }
    }

    fn dim_of_power(&mut self, exponent: &Expr, base: Dim) -> Dim {
        if !base.known {
            return Dim::unknown();
        }
        if base.quantity.is_dimensionless() {
            return Dim::of(Quantity::dimensionless(1.0));
        }
        if let Expr::Num { value, .. } = exponent {
            return Dim::of(base.quantity.powf(*value));
        }
        let message = format!(
            "{}: a dimensional quantity [{}] is raised to a non-constant exponent.",
            self.current_equation,
            base.quantity.dimension_string()
        );
        self.warn(message);
        Dim::unknown()
    }

    // -- calls ---------------------------------------------------------------

    fn dim_of_call(&mut self, function: &str, args: &[Expr]) -> Dim {
        // User TABLE/FUNCTION with a declared output unit carries those dims.
        if let Some(declared) = self.function_dims.get(function).copied() {
            return Dim::of(declared);
        }
        // Property calls carry the dimensions of the requested output.
        if function.starts_with("prop$") {
            return property_dim(function);
        }
        // Eigendecomposition: matrix entries may carry any units. Eigenvalues
        // inherit the entry dimensions; eigenvector components are dimensionless.
        if function.starts_with("eigen$") {
            if function.starts_with("eigen$vec") {
                return Dim::of(Quantity::dimensionless(1.0));
            }
            for arg in args {
                let d = self.dim_of(arg);
                if d.known {
                    return d;
                }
            }
            return Dim::unknown();
        }
        // (det$ carries [u]^n for a uniform [u] matrix, which Dim cannot
        // express without a power op — agnostic like the rest; the n<=3
        // closed-form expansion still derives exact dimensions.)
        if AGNOSTIC_CALL_PREFIXES
            .iter()
            .any(|p| function.starts_with(p))
        {
            return Dim::unknown();
        }
        if DIMENSIONLESS_CALL_PREFIXES
            .iter()
            .any(|p| function.starts_with(p))
        {
            return Dim::of(Quantity::dimensionless(1.0));
        }
        match function {
            "abs" | "real" | "imag" => self.dim_of_first(args),
            // ArrayElmt returns one element, so it carries the array's units.
            "arrayelmt" => self.dim_of_first(args),
            // Radiation view factors take length arguments and return a
            // dimensionless ratio; do not warn on the length-valued arguments.
            "viewfactor_perp" | "viewfactor_plates" | "viewfactor_disks" => {
                Dim::of(Quantity::dimensionless(1.0))
            }
            // Heisler transient-conduction results are dimensionless ratios; the
            // geometry string and Bi/Fo/x* arguments carry no units to check.
            "heisler_temp" | "heisler_q" => Dim::of(Quantity::dimensionless(1.0)),
            // Ideal-gas compressible-flow relations: every output is a
            // dimensionless property ratio or an angle in radians (also
            // dimensionless in SI); the Mach/k/angle arguments carry no units.
            "t0_t" | "isen_t0_t" | "p0_p" | "isen_p0_p" | "rho0_rho" | "isen_rho0_rho"
            | "a_astar" | "isen_a_astar" | "mach_a_astar" | "m2_shock" | "mach_shock"
            | "p2_p1_shock" | "t2_t1_shock" | "rho2_rho1_shock" | "p02_p01_shock"
            | "rayleigh_t0_t0star" | "rayleigh_t_tstar" | "rayleigh_p_pstar"
            | "rayleigh_p0_p0star" | "fanno_t_tstar" | "fanno_p_pstar" | "fanno_p0_p0star"
            | "fanno_fld" | "prandtlmeyer" | "prandtl_meyer" | "mach_prandtlmeyer"
            | "machangle" | "theta_oblique" | "beta_oblique" => {
                Dim::of(Quantity::dimensionless(1.0))
            }
            // Heat-exchanger effectiveness, NTU and fin efficiency are
            // dimensionless; the leading arrangement string carries no units.
            "hx_effectiveness" | "hx_epsilon" | "hx_ntu" | "fin_efficiency" => {
                Dim::of(Quantity::dimensionless(1.0))
            }
            // Flow resistance: friction factor and Reynolds number are
            // dimensionless; a minor (fitting) loss is a pressure [Pa].
            "friction_factor" | "darcy_friction" | "reynolds" | "re_number" => {
                Dim::of(Quantity::dimensionless(1.0))
            }
            "minor_loss" => eos_dim("Pa"),
            // Pneumatics: ISO 6358 returns a mass flow rate [kg/s]; the sonic
            // conductance / pressure-ratio arguments are not policed.
            "iso6358" => eos_dim("kg/s"),
            // HX sizing correlations: film coefficients [W/m^2/K], overall UA
            // [W/K], and friction pressure drops [Pa] (flow/geometry args unpoliced).
            "htc_1phase" | "htc_evap" | "htc_cond" | "htc_extair" => eos_dim("W/m^2/K"),
            "ua_hx" => eos_dim("W/K"),
            "dp_1phase" | "dp_2phase" | "dp_mueller_steinhagen" | "dp_ms" | "dp_compact_core" => {
                eos_dim("Pa")
            }
            // External/free-convection Nusselt numbers, free-flow ratio and
            // surface efficiency are dimensionless; convective area [m^2], D_h [m].
            "nu_zukauskas"
            | "nu_colburn"
            | "nu_churchill_chu"
            | "nu_blend"
            | "nu_tubebank"
            | "nu_hilpert"
            | "nu_plate"
            | "nu_gungor_winterton"
            | "nu_traviss"
            | "j_fin"
            | "f_fin"
            | "hx_sigma"
            | "hx_eta_surf" => Dim::of(Quantity::dimensionless(1.0)),
            "hx_dh" | "hx_fin_len" => eos_dim("m"),
            "hx_aconv" | "hx_area_direct" | "hx_area_indirect" => eos_dim("m^2"),
            "dp_gravity" | "dp_2phase_avg" => eos_dim("Pa"),
            "mass_flux" => eos_dim("kg/m^2/s"),
            // Two-phase flow: the Martinelli parameter and its multiplier are
            // both dimensionless (quality / property-ratio arguments unpoliced).
            "lm_phi2" | "lm_martinelli_tt" => Dim::of(Quantity::dimensionless(1.0)),
            // Void fraction and the Friedel multiplier are dimensionless; the
            // momentum flux is a pressure [Pa] (quality/property arguments unpoliced).
            "void_homogeneous" | "void_zivi" | "void_rouhani" | "friedel_phi2" => {
                Dim::of(Quantity::dimensionless(1.0))
            }
            "momentum_flux" => eos_dim("Pa"),
            // Nusselt numbers and the Chen factors / zone ramp are all dimensionless.
            "nu_dittus_boelter"
            | "nu_gnielinski"
            | "chen_f"
            | "chen_s"
            | "nu_shah"
            | "nu_cavallini_zecchin"
            | "zone_ramp" => Dim::of(Quantity::dimensionless(1.0)),
            // ISA standard atmosphere: SI property units (altitude argument unpoliced).
            "isa_t" => eos_dim("K"),
            "isa_p" => eos_dim("Pa"),
            "isa_rho" => eos_dim("kg/m^3"),
            // LMTD returns a temperature difference, inheriting the units of its
            // terminal-difference arguments (which the checker does not police).
            "lmtd" => self.dim_of_first(args),
            // Cubic-EOS backend: outputs carry their SI property units; the
            // leading fluid$/model$/phase$ strings carry none.
            "eos_z" => Dim::of(Quantity::dimensionless(1.0)),
            "eos_pressure" | "eos_psat" => eos_dim("Pa"),
            "eos_volume" => eos_dim("m^3/kg"),
            "eos_density" => eos_dim("kg/m^3"),
            "eos_enthalpy" => eos_dim("J/kg"),
            "eos_entropy" => eos_dim("J/kg-K"),
            // Combustion thermochemistry & ideal-gas mixtures (string-keyed
            // species/composition args carry no units to check).
            "adiabaticflametemp" | "adiabaticflametemperature" | "flametemp" => eos_dim("K"),
            "mix_mw" | "mix_molarmass" => eos_dim("kg/mol"),
            "mix_cp" => eos_dim("J/kg-K"),
            "mix_enthalpy" => eos_dim("J/kg"),
            "mix_entropy" => eos_dim("J/kg-K"),
            "mix_viscosity" => eos_dim("Pa-s"),
            "mix_conductivity" => eos_dim("W/m-K"),
            // Equilibrium mole fraction is dimensionless; the equilibrium flame
            // temperature carries kelvin.
            "eq_molefraction" => Dim::of(Quantity::dimensionless(1.0)),
            "adiabaticflametempeq" | "flametemp_eq" => eos_dim("K"),
            // Wiebe burned-fraction and its rate are dimensionless.
            "wiebe" | "wiebe_rate" => Dim::of(Quantity::dimensionless(1.0)),
            // TABLE lookup/interpolation: the table name is a string and the
            // arguments/result carry the table's own units, which the checker
            // does not track — stay agnostic rather than warn.
            "interpolate" | "interpolate1" | "interpolate2d" | "lookup" | "lookuprow"
            | "nlookuprows" | "differentiate" | "differentiate1" | "dtable" | "dtable1" => {
                Dim::unknown()
            }
            // Parametric-table accessors carry the referenced column's units,
            // which the checker does not track — stay agnostic.
            "tablerun#" | "tablerun" | "nparametricruns" | "tablevalue" | "tablesum"
            | "tableavg" | "tablemin" | "tablemax" | "tablestddev" | "integralvalue" => {
                Dim::unknown()
            }
            "stagnationtemp" => {
                let t_dim = self.dim_of_first(args);
                match UnitRegistry::parse("K") {
                    Ok(kelvin) => {
                        if t_dim.known && !t_dim.quantity.same_dimensions_as(&kelvin) {
                            let message = format!(
                                "{}: stagnationtemp temperature argument must have temperature \
                                 units (e.g. K, C), got [{}].",
                                self.current_equation,
                                t_dim.quantity.dimension_string()
                            );
                            self.warn(message);
                        }
                        Dim::of(kelvin)
                    }
                    Err(_) => Dim::unknown(),
                }
            }
            "stagnationpres" => {
                let p_dim = self.dim_of_first(args);
                match UnitRegistry::parse("Pa") {
                    Ok(pascal) => {
                        if p_dim.known && !p_dim.quantity.same_dimensions_as(&pascal) {
                            let message = format!(
                                "{}: stagnationpres pressure argument must have pressure units \
                                 (e.g. Pa, kPa), got [{}].",
                                self.current_equation,
                                p_dim.quantity.dimension_string()
                            );
                            self.warn(message);
                        }
                        Dim::of(pascal)
                    }
                    Err(_) => Dim::unknown(),
                }
            }
            "min" | "max" => {
                let first = self.dim_of_first(args);
                let second = if args.len() > 1 {
                    self.dim_of(&args[1])
                } else {
                    first
                };
                if first.known
                    && second.known
                    && !first.quantity.same_dimensions_as(&second.quantity)
                {
                    let message = format!(
                        "{}: {function} arguments have different units [{}] vs [{}].",
                        self.current_equation,
                        first.quantity.dimension_string(),
                        second.quantity.dimension_string()
                    );
                    self.warn(message);
                }
                if first.known {
                    first
                } else {
                    second
                }
            }
            "sqrt" => {
                let arg = self.dim_of_first(args);
                if arg.known {
                    Dim::of(arg.quantity.powf(0.5))
                } else {
                    Dim::unknown()
                }
            }
            "cbrt" => {
                let arg = self.dim_of_first(args);
                if arg.known {
                    Dim::of(arg.quantity.powf(1.0 / 3.0))
                } else {
                    Dim::unknown()
                }
            }
            // Integral(f, t, a, b) has the dimensions of f*t; the checker does
            // not track them, so stay agnostic instead of warning.
            "integral" | "gaussintegral" => Dim::unknown(),
            // Descriptive statistics carry the data's units (the data are the
            // arguments; for percentile the first argument is the percentile p).
            "mean" | "median" | "stdev" | "stddev" | "std" | "variance" | "var" | "rms" => {
                self.dim_of_first(args)
            }
            "percentile" => match args.last() {
                Some(last) => self.dim_of(last),
                None => Dim::unknown(),
            },
            // Distribution CDF/PDF/quantile: result is a dimensionless
            // probability/quantile; do not constrain the (possibly dimensioned)
            // inputs.
            "normalcdf" | "normalpdf" | "normalinvcdf" => Dim::unknown(),
            _ => {
                // sin, cos, tan, exp, ln, log10, log2, arc*: argument must be
                // dimensionless.
                if let Some(first) = args.first() {
                    let arg = self.dim_of(first);
                    if arg.known && !arg.quantity.is_dimensionless() {
                        let message = format!(
                            "{}: the argument of {function} must be dimensionless but has \
                             units [{}].",
                            self.current_equation,
                            arg.quantity.dimension_string()
                        );
                        self.warn(message);
                    }
                }
                Dim::of(Quantity::dimensionless(1.0))
            }
        }
    }

    /// `dimOf(args.get(0))`, degrading to agnostic when the call has no
    /// arguments (where the Java would throw).
    fn dim_of_first(&mut self, args: &[Expr]) -> Dim {
        match args.first() {
            Some(first) => self.dim_of(first),
            None => Dim::unknown(),
        }
    }
}

/// Null-propagating multiplicative combination of two analyzed terms.
/// Port of `UnitChecker.combine`; `sign` is `1.0` for `*`, `-1.0` for `/`.
fn combine(a: Option<DimTerm>, b: Option<DimTerm>, sign: f64) -> Option<DimTerm> {
    let (a, b) = (a?, b?);
    let mut dims = [0.0; DIMENSIONS];
    for ((d, x), y) in dims.iter_mut().zip(a.dims.iter()).zip(b.dims.iter()) {
        *d = x + sign * y;
    }
    Some(DimTerm {
        dims,
        unknown_exponent: a.unknown_exponent + sign * b.unknown_exponent,
    })
}

/// Treats an unresolved bare numeric literal as dimensionless when it appears
/// as a multiplicative factor. Other wildcards (unit-less variables) stay
/// unknown so genuine ambiguity is not masked. Port of `UnitChecker.asFactor`.
fn as_factor(operand: &Expr, resolved: Dim) -> Dim {
    if !resolved.known && matches!(operand, Expr::Num { unit: None, .. }) {
        return Dim::of(Quantity::dimensionless(1.0));
    }
    resolved
}

/// Safe SI-unit dimension for a cubic-EOS output, agnostic if unparseable.
/// Port of `UnitChecker.eosDim`.
fn eos_dim(unit: &str) -> Dim {
    match UnitRegistry::parse(unit) {
        Ok(q) => Dim::of(q),
        Err(_) => Dim::unknown(),
    }
}

/// Property calls carry the dimensions of the requested output.
/// Port of `UnitChecker.propertyDim`; `encoded` is `prop$<output>$<fluid>`.
fn property_dim(encoded: &str) -> Dim {
    let unit = encoded.split('$').nth(1).and_then(property_unit);
    match unit {
        None => Dim::unknown(),
        Some("-") => Dim::of(Quantity::dimensionless(1.0)),
        Some(u) => match UnitRegistry::parse(u) {
            Ok(q) => Dim::of(q),
            Err(_) => Dim::unknown(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_document;

    /// Parse a document and return its equations, owned.
    fn eqs(source: &str) -> Vec<Equation> {
        let doc = parse_document(source).expect("test document must parse");
        doc.equations().into_iter().cloned().collect()
    }

    fn units(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn no_units() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    // -- inference ----------------------------------------------------------

    #[test]
    fn infers_units_for_multiplicative_definitions() {
        let report = check_units(&eqs("F = m * a"), &units(&[("m", "kg"), ("a", "m/s^2")]));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("f").map(String::as_str), Some("N"));
    }

    #[test]
    fn fixpoint_iteration_propagates_chains_across_equation_order() {
        // P needs F, but F's equation comes second: only a second pass can
        // resolve P. Equations are order-independent.
        let report = check_units(
            &eqs("P = F / A\nF = m * g"),
            &units(&[("m", "kg"), ("g", "m/s^2"), ("A", "m^2")]),
        );
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("f").map(String::as_str), Some("N"));
        assert_eq!(report.inferred.get("p").map(String::as_str), Some("Pa"));
    }

    #[test]
    fn rearrangement_solves_for_a_multiplicative_factor() {
        // F and g known: m = F / g by dimensional rearrangement.
        let report = check_units(&eqs("F = m * g"), &units(&[("F", "N"), ("g", "m/s^2")]));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("m").map(String::as_str), Some("kg"));
    }

    #[test]
    fn power_rearrangement_scales_the_unknown_exponent() {
        // E = 0.5*m*v^2 with E [J], m [kg]: v = sqrt(J/kg) = m/s.
        let report = check_units(
            &eqs("E = 0.5 * m * v^2"),
            &units(&[("E", "J"), ("m", "kg")]),
        );
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("v").map(String::as_str), Some("m/s"));
    }

    #[test]
    fn additive_partner_grounds_unknowns_in_sums() {
        // Two unknowns (Tout, dT): the multiplicative solver cannot run, but
        // dT + Tin forces dT = K, and the next pass resolves Tout.
        let report = check_units(&eqs("Tout = Tin + dT"), &units(&[("Tin", "K")]));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("dt").map(String::as_str), Some("K"));
        assert_eq!(report.inferred.get("tout").map(String::as_str), Some("K"));
    }

    #[test]
    fn var_to_var_chains_resolve_over_passes() {
        let report = check_units(&eqs("c = b\nb = a"), &units(&[("a", "m")]));
        assert_eq!(report.inferred.get("b").map(String::as_str), Some("m"));
        assert_eq!(report.inferred.get("c").map(String::as_str), Some("m"));
    }

    #[test]
    fn bare_literals_are_dimensionless_scale_factors() {
        let report = check_units(&eqs("F2 = 2 * F1"), &units(&[("F1", "N")]));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("f2").map(String::as_str), Some("N"));
    }

    #[test]
    fn dimensionless_base_with_nonconstant_exponent_keeps_context_units() {
        // T2 = T1 * (P2/P1)^e: the pressure ratio is dimensionless so the
        // non-constant exponent is harmless and T2 keeps kelvin.
        let report = check_units(
            &eqs("T2 = T1 * (P2/P1) ^ e"),
            &units(&[("T1", "K"), ("P1", "Pa"), ("P2", "Pa"), ("e", "-")]),
        );
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("t2").map(String::as_str), Some("K"));
    }

    #[test]
    fn undeclared_wildcards_stay_silent_and_uninferred() {
        let report = check_units(&eqs("y = x * 2"), &no_units());
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert!(report.inferred.is_empty(), "{:?}", report.inferred);
    }

    #[test]
    fn inferred_keys_are_lowercase_and_declared_keys_case_fold() {
        let report = check_units(&eqs("F = M * A"), &units(&[("M", "kg"), ("A", "m/s^2")]));
        assert_eq!(report.inferred.get("f").map(String::as_str), Some("N"));
        assert!(!report.inferred.contains_key("F"));
    }

    // -- warnings -----------------------------------------------------------

    #[test]
    fn lhs_rhs_mismatch_warns_with_the_java_sentence() {
        let report = check_units(&eqs("x = 2 [m]"), &units(&[("x", "s")]));
        assert_eq!(
            report.warnings,
            vec!["x = 2 [m]: the units of the left side [s] do not match the right side [m]."]
        );
    }

    #[test]
    fn add_subtract_mismatch_warns_and_result_keeps_the_left_units() {
        let report = check_units(&eqs("y = 1 [m] + 1 [s]"), &no_units());
        assert_eq!(
            report.warnings,
            vec!["y = 1 [m] + 1 [s]: cannot add/subtract [m] and [s]."]
        );
        // The mismatch still infers: the sum yields its left operand's units.
        assert_eq!(report.inferred.get("y").map(String::as_str), Some("m"));
    }

    #[test]
    fn inconsistent_documents_still_return_inferences() {
        let report = check_units(
            &eqs("F = m * a\nz = 3 [m]"),
            &units(&[("m", "kg"), ("a", "m/s^2"), ("z", "K")]),
        );
        assert_eq!(
            report.warnings,
            vec!["z = 3 [m]: the units of the left side [K] do not match the right side [m]."]
        );
        assert_eq!(report.inferred.get("f").map(String::as_str), Some("N"));
    }

    #[test]
    fn explicit_dimensionless_marker_participates_in_mismatch_warnings() {
        let report = check_units(&eqs("x = 3 [m]"), &units(&[("x", "-")]));
        assert_eq!(
            report.warnings,
            vec!["x = 3 [m]: the units of the left side [-] do not match the right side [m]."]
        );
    }

    #[test]
    fn dimensional_base_with_nonconstant_exponent_warns() {
        let report = check_units(&eqs("y = x ^ n"), &units(&[("x", "m")]));
        assert_eq!(
            report.warnings,
            vec!["y = x ^ n: a dimensional quantity [m] is raised to a non-constant exponent."]
        );
        assert!(report.inferred.is_empty());
    }

    #[test]
    fn compound_dimension_strings_use_base_exponent_form() {
        // Density vs mass: the warning quotes "kg m^-3".
        let report = check_units(&eqs("rho = 3 [kg]"), &units(&[("rho", "kg/m^3")]));
        assert_eq!(
            report.warnings,
            vec![
                "rho = 3 [kg]: the units of the left side [kg m^-3] do not match the right \
                 side [kg]."
            ]
        );
    }

    // -- intrinsics ----------------------------------------------------------

    #[test]
    fn trig_demands_a_dimensionless_argument() {
        let report = check_units(&eqs("q = sin(t)"), &units(&[("t", "s")]));
        assert_eq!(
            report.warnings,
            vec!["q = sin(t): the argument of sin must be dimensionless but has units [s]."]
        );
        // The result is still dimensionless, so q resolves to "-".
        assert_eq!(report.inferred.get("q").map(String::as_str), Some("-"));
    }

    #[test]
    fn trig_of_dimensionless_is_silent() {
        let report = check_units(&eqs("q = sin(theta)"), &units(&[("theta", "-")]));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("q").map(String::as_str), Some("-"));
    }

    #[test]
    fn sqrt_halves_dimension_exponents() {
        let report = check_units(
            &eqs("v = sqrt(2 * g * h)"),
            &units(&[("g", "m/s^2"), ("h", "m")]),
        );
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("v").map(String::as_str), Some("m/s"));
    }

    #[test]
    fn cbrt_thirds_dimension_exponents() {
        let report = check_units(&eqs("L = cbrt(V)"), &units(&[("V", "m^3")]));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("l").map(String::as_str), Some("m"));
    }

    #[test]
    fn abs_carries_its_argument_units() {
        let report = check_units(&eqs("y = abs(x)"), &units(&[("x", "m")]));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("y").map(String::as_str), Some("m"));
    }

    #[test]
    fn statistics_carry_the_data_units() {
        let report = check_units(
            &eqs("mu = mean(a, b)\npct = percentile(90, a)"),
            &units(&[("a", "K"), ("b", "K")]),
        );
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("mu").map(String::as_str), Some("K"));
        assert_eq!(report.inferred.get("pct").map(String::as_str), Some("K"));
    }

    #[test]
    fn min_max_mismatch_warns_and_keeps_the_first_argument() {
        let report = check_units(&eqs("w = min(a, b)"), &units(&[("a", "m"), ("b", "s")]));
        assert_eq!(
            report.warnings,
            vec!["w = min(a, b): min arguments have different units [m] vs [s]."]
        );
        assert_eq!(report.inferred.get("w").map(String::as_str), Some("m"));
    }

    #[test]
    fn min_with_a_single_argument_does_not_warn() {
        let report = check_units(&eqs("w = min(a)"), &units(&[("a", "m")]));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("w").map(String::as_str), Some("m"));
    }

    #[test]
    fn table_lookups_stay_agnostic_rather_than_warning() {
        let report = check_units(&eqs("y = interpolate('tbl', x)"), &units(&[("x", "m")]));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert!(!report.inferred.contains_key("y"));
    }

    #[test]
    fn elementwise_operators_are_dimension_agnostic() {
        let report = check_units(&eqs("z = a .* b"), &units(&[("a", "m"), ("b", "m")]));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert!(!report.inferred.contains_key("z"));
    }

    #[test]
    fn stagnationtemp_polices_its_argument_and_returns_kelvin() {
        let equations = vec![Equation::new(
            Expr::var("t0"),
            Expr::call("stagnationtemp", vec![Expr::var("p")]),
            "t0 = stagnationtemp(p)",
        )];
        let report = check_units(&equations, &units(&[("p", "Pa")]));
        assert_eq!(
            report.warnings,
            vec![
                "t0 = stagnationtemp(p): stagnationtemp temperature argument must have \
                 temperature units (e.g. K, C), got [kg m^-1 s^-2]."
            ]
        );
        assert_eq!(report.inferred.get("t0").map(String::as_str), Some("K"));
    }

    #[test]
    fn stagnationpres_polices_its_argument_and_returns_pascal() {
        let equations = vec![Equation::new(
            Expr::var("p0"),
            Expr::call("stagnationpres", vec![Expr::var("t")]),
            "p0 = stagnationpres(t)",
        )];
        let report = check_units(&equations, &units(&[("t", "K")]));
        assert_eq!(
            report.warnings,
            vec![
                "p0 = stagnationpres(t): stagnationpres pressure argument must have pressure \
                 units (e.g. Pa, kPa), got [K]."
            ]
        );
        assert_eq!(report.inferred.get("p0").map(String::as_str), Some("Pa"));
    }

    #[test]
    fn comparisons_logicals_and_not_are_dimensionless() {
        let compare = Equation::new(
            Expr::var("c"),
            Expr::Compare {
                op: crate::ast::CmpOp::Gt,
                left: Box::new(Expr::var("a")),
                right: Box::new(Expr::var("b")),
            },
            "c = (a > b)",
        );
        let not = Equation::new(
            Expr::var("n"),
            Expr::Not(Box::new(Expr::var("c"))),
            "n = not c",
        );
        let report = check_units(&[compare, not], &units(&[("a", "m"), ("b", "m")]));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("c").map(String::as_str), Some("-"));
        assert_eq!(report.inferred.get("n").map(String::as_str), Some("-"));
    }

    #[test]
    fn property_calls_carry_their_output_units() {
        let equations = vec![
            Equation::new(
                Expr::var("p"),
                Expr::call("prop$pressure$Water", vec![]),
                "p = pressure(Water, ...)",
            ),
            Equation::new(
                Expr::var("x"),
                Expr::call("prop$quality$Water", vec![]),
                "x = quality(Water, ...)",
            ),
            Equation::new(
                Expr::var("z"),
                Expr::call("prop$bogus$Water", vec![]),
                "z = bogus(Water, ...)",
            ),
        ];
        let report = check_units(&equations, &no_units());
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("p").map(String::as_str), Some("Pa"));
        assert_eq!(report.inferred.get("x").map(String::as_str), Some("-"));
        assert!(!report.inferred.contains_key("z"));
    }

    #[test]
    fn eigenvalues_inherit_entry_dims_and_eigenvectors_are_dimensionless() {
        let equations = vec![
            Equation::new(
                Expr::var("lambda"),
                Expr::call("eigen$val1", vec![Expr::var("a")]),
                "lambda = eigenval(A, 1)",
            ),
            Equation::new(
                Expr::var("vc"),
                Expr::call("eigen$vec1", vec![Expr::var("a")]),
                "vc = eigenvec(A, 1)",
            ),
        ];
        let report = check_units(&equations, &units(&[("a", "Pa")]));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(
            report.inferred.get("lambda").map(String::as_str),
            Some("Pa")
        );
        assert_eq!(report.inferred.get("vc").map(String::as_str), Some("-"));
    }

    #[test]
    fn control_prefixed_calls_are_dimensionless_and_matrix_calls_agnostic() {
        let equations = vec![
            Equation::new(
                Expr::var("k"),
                Expr::call("lqr$k11", vec![Expr::var("a")]),
                "k = lqr(...)",
            ),
            Equation::new(
                Expr::var("d"),
                Expr::call("det$1", vec![Expr::var("a")]),
                "d = det(A)",
            ),
        ];
        let report = check_units(&equations, &units(&[("a", "Pa")]));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("k").map(String::as_str), Some("-"));
        assert!(!report.inferred.contains_key("d"));
    }

    #[test]
    fn empty_argument_lists_degrade_instead_of_panicking() {
        let equations = vec![
            Equation::new(Expr::var("a"), Expr::call("abs", vec![]), "a = abs()"),
            Equation::new(Expr::var("s"), Expr::call("sin", vec![]), "s = sin()"),
            Equation::new(Expr::var("m"), Expr::call("min", vec![]), "m = min()"),
            Equation::new(
                Expr::var("p"),
                Expr::call("percentile", vec![]),
                "p = percentile()",
            ),
        ];
        let report = check_units(&equations, &no_units());
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        // sin() is still dimensionless by its result type; the rest stay agnostic.
        assert_eq!(report.inferred.get("s").map(String::as_str), Some("-"));
        assert!(!report.inferred.contains_key("a"));
        assert!(!report.inferred.contains_key("m"));
        assert!(!report.inferred.contains_key("p"));
    }

    // -- unknown units degrade gracefully -----------------------------------

    #[test]
    fn unknown_declared_unit_warns_and_leaves_a_wildcard() {
        let report = check_units(&eqs("x = y"), &units(&[("x", "blorp")]));
        assert_eq!(
            report.warnings,
            vec!["Variable x: Unknown unit: 'blorp' in 'blorp'"]
        );
        assert!(report.inferred.is_empty(), "{:?}", report.inferred);
    }

    #[test]
    fn variable_warning_preserves_the_declared_name_case() {
        let report = check_units(&eqs("Xx = y"), &units(&[("Xx", "blorp")]));
        assert_eq!(
            report.warnings,
            vec!["Variable Xx: Unknown unit: 'blorp' in 'blorp'"]
        );
    }

    #[test]
    fn unknown_literal_unit_warns_inside_the_equation() {
        // The parser keeps an unresolvable literal unit verbatim; the checker
        // turns it into a warning here.
        let report = check_units(&eqs("x = 5 [blorp]"), &no_units());
        assert_eq!(
            report.warnings,
            vec!["x = 5 [blorp]: Unknown unit: 'blorp' in 'blorp'"]
        );
        assert!(report.inferred.is_empty(), "{:?}", report.inferred);
    }

    #[test]
    fn unknown_unit_inside_an_expression_names_the_failing_factor() {
        let report = check_units(&eqs("x = 5 [blorp/s]"), &no_units());
        assert_eq!(
            report.warnings,
            vec!["x = 5 [blorp/s]: Unknown unit: 'blorp' in 'blorp/s'"]
        );
    }

    #[test]
    fn malformed_unit_expressions_warn_as_cannot_parse() {
        let report = check_units(&eqs("x = y"), &units(&[("x", "m^2K")]));
        assert_eq!(
            report.warnings,
            vec!["Variable x: Cannot parse unit: 'm^2K' in 'm^2K'"]
        );
    }

    // -- function output / argument units ------------------------------------

    #[test]
    fn declared_function_output_units_ground_calls() {
        // Q must already be grounded: with two unknowns (dp, Q) the Java
        // derivation skips the equation entirely, so parity means no inference.
        let report = check_units_full(
            &eqs("dp = fancurve(Q)"),
            &units(&[("Q", "m^3/s")]),
            &units(&[("fancurve", "Pa")]),
            &BTreeMap::new(),
        );
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("dp").map(String::as_str), Some("Pa"));
    }

    #[test]
    fn ungrounded_call_argument_blocks_output_inference_as_in_java() {
        // Same document without Q declared: dp and Q are both unknown, the
        // single-unknown rearrangement never fires, and nothing is inferred.
        let report = check_units_full(
            &eqs("dp = fancurve(Q)"),
            &no_units(),
            &units(&[("fancurve", "Pa")]),
            &BTreeMap::new(),
        );
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert!(report.inferred.is_empty(), "{:?}", report.inferred);
    }

    #[test]
    fn declared_function_argument_units_ground_argument_variables() {
        let mut input_units: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
        input_units.insert("fancurve".to_string(), vec![Some("m^3/s".to_string())]);
        let report = check_units_full(
            &eqs("dp = fancurve(Q)"),
            &no_units(),
            &units(&[("fancurve", "Pa")]),
            &input_units,
        );
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("q").map(String::as_str), Some("m^3/s"));
        assert_eq!(report.inferred.get("dp").map(String::as_str), Some("Pa"));
    }

    #[test]
    fn argument_unit_rearrangement_grounds_compound_arguments() {
        // fancurve(Vair/f_rpm) with arg unit m^3/s: the synthetic equation
        // Vair/f_rpm = X[m^3/s] solves for Vair once f_rpm is known.
        let mut input_units: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
        input_units.insert("fancurve".to_string(), vec![Some("m^3/s".to_string())]);
        let report = check_units_full(
            &eqs("dp = fancurve(Vair / f_rpm)"),
            &units(&[("f_rpm", "rad/s")]),
            &BTreeMap::new(),
            &input_units,
        );
        // Vair / (rad/s) = m^3/s  =>  Vair = m^3/s^2.
        assert_eq!(
            report.inferred.get("vair").map(String::as_str),
            Some("m^3/s^2")
        );
    }

    #[test]
    fn bad_declared_function_unit_is_silently_ignored() {
        let report = check_units_full(
            &eqs("y = f(x)"),
            &units(&[("x", "-")]),
            &units(&[("f", "blorp")]),
            &BTreeMap::new(),
        );
        // No warning for the bad declaration; f falls back to the default
        // (dimensionless) intrinsic rule, so y still resolves to "-".
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.inferred.get("y").map(String::as_str), Some("-"));
    }

    // -- property table -------------------------------------------------------

    #[test]
    fn property_unit_matches_the_java_table() {
        assert_eq!(property_unit("temperature"), Some("K"));
        assert_eq!(property_unit("pressure"), Some("Pa"));
        assert_eq!(property_unit("enthalpy"), Some("J/kg"));
        assert_eq!(property_unit("entropy"), Some("J/kg-K"));
        assert_eq!(property_unit("density"), Some("kg/m^3"));
        assert_eq!(property_unit("volume"), Some("m^3/kg"));
        assert_eq!(property_unit("viscosity"), Some("Pa-s"));
        assert_eq!(property_unit("conductivity"), Some("W/m-K"));
        assert_eq!(property_unit("soundspeed"), Some("m/s"));
        assert_eq!(property_unit("molarmass"), Some("kg/mol"));
        assert_eq!(property_unit("heatingvalue"), Some("J/kg"));
        assert_eq!(property_unit("quality"), Some("-"));
        assert_eq!(property_unit("relhum"), Some("-"));
        assert_eq!(property_unit("nonsense"), None);
    }

    // -- report shape ---------------------------------------------------------

    #[test]
    fn variable_unit_warnings_precede_equation_warnings() {
        let report = check_units(&eqs("x = 2 [m]"), &units(&[("x", "s"), ("zz", "blorp")]));
        assert_eq!(
            report.warnings,
            vec![
                "Variable zz: Unknown unit: 'blorp' in 'blorp'".to_string(),
                "x = 2 [m]: the units of the left side [s] do not match the right side [m]."
                    .to_string(),
            ]
        );
    }

    #[test]
    fn derived_map_excludes_declared_variables() {
        let report = check_units(&eqs("F = m * a"), &units(&[("m", "kg"), ("a", "m/s^2")]));
        assert!(!report.inferred.contains_key("m"));
        assert!(!report.inferred.contains_key("a"));
        assert_eq!(report.inferred.len(), 1);
    }
}
