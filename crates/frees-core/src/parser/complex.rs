//! Complex-number expansion: splits equations and variables into real (`_r`)
//! and imaginary (`_i`) parts.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/parser/ComplexExpansion.java`
//! (618 LOC), plus the trigger rule its callers apply.
//!
//! # When expansion triggers (verified against `EquationSystemSolver.java`)
//!
//! The Java engine gates the pass on the **complex-mode setting alone**, never
//! on the presence of an imaginary literal:
//!
//! * `settings.complexMode()` **on** → `ComplexExpansion.expand(...)` runs on
//!   every equation (`check` line 163, `solve` line 327, `solvePermissive`
//!   line 703, `solveTable` line 1820). A purely **real** document in complex
//!   mode is still expanded — every variable `x` becomes `x_r`/`x_i`, and
//!   imaginary parts no equation determines are pinned with
//!   `x_i = 0 (default complex real)` equations.
//! * `settings.complexMode()` **off** + an imaginary literal (`1i`, `2j`)
//!   anywhere → hard error before anything else runs
//!   (`requireComplexModeForImaginaryLiterals`, lines 246–254): *"The
//!   equations contain complex literals (e.g. 1i): enable Complex mode to
//!   solve them."* Nothing is ever split in real mode.
//! * `settings.complexMode()` **off**, no imaginary literal → the equations
//!   pass through untouched (the golden corpus freezes this pipeline).
//!
//! [`expand_complex`] implements all three arms so the engine can call it
//! unconditionally. [`expand_with_display_names`] is the full Java
//! `expand(equations, displayNames)` — the Java method also mutates the
//! parse result's display-name map (`x_r` → `X_r` when `x` displayed as `X`),
//! which the frozen `expand_complex` signature cannot carry; engine wiring
//! that wants display names calls the wider entry point.
//!
//! # How each operation splits (all verified against the Java source)
//!
//! With `z = a + b·i`, `w = c + d·i`:
//!
//! * `z ± w` → `(a ± c) + (b ± d)·i`
//! * `z * w` → `(a·c − b·d) + (a·d + b·c)·i`
//! * `z / w` → `((a·c + b·d) + (b·c − a·d)·i) / (c² + d²)`
//! * `z ^ w` → de Moivre in polar form: `|z^w| = r^c · e^(−d·θ)` and
//!   `arg(z^w) = c·θ + d·ln r`, with `r = √(a²+b²)`, `θ = atan2(b, a)`.
//!   When `d` is the **literal zero** (a real exponent, the common case) the
//!   `e`-term and `ln r`-term are omitted so `z = 0` does not produce
//!   `ln(0)·0 = NaN` during iteration.
//! * Elementary functions of a complex argument: `sin`, `cos`, `exp`, `ln`,
//!   `sqrt`, `abs` (= magnitude), `conj`, `real`, `imag`, `magnitude`,
//!   `angle`/`anglerad`, `angledeg`, `cis` — each with its explicit rule.
//!   Any **other** function of a complex argument is rejected (`Im tan(z) ≠
//!   tan(Im z)`; silently mapping parts through is mathematically wrong).
//! * Comparisons, logicals and `not` evaluate to 1.0/0.0: their real part
//!   maps the operands' real parts, their imaginary part is literally `0`.
//! * `a[i]` → `a_r[i]` / `a_i[i]`: the **name** is suffixed, the indices pass
//!   through unchanged (Java `ComplexExpansion` lines 501/600).
//!
//! # Deliberate divergences from Java (all unreachable-in-practice corners)
//!
//! * Matrix/element-wise operators (`\`, `.*`, `./`, `.\`, `.^`) inside a
//!   complex expansion throw a bare `IllegalStateException` ("Unknown
//!   operator") in Java — an HTTP 500. Here they are a clean
//!   [`FreesError::Parse`].
//! * A supported function called with **no** arguments hits
//!   `IndexOutOfBoundsException` in Java; here it is a clean
//!   [`FreesError::Evaluation`].
//! * An expanded equation can mention a variable outside the `_r`/`_i`
//!   universe only via an array **index** (`a[n]` keeps bare `n`); Java's
//!   JGraphT `addEdge` throws on the unknown vertex, here the variable simply
//!   takes no part in the matching.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use crate::ast::{BinOp, Equation, Expr};
use crate::diag::{FreesError, Result};

const ATAN2: &str = "atan2";
const FN_MAGNITUDE: &str = "magnitude";
const FN_ANGLE: &str = "angle";
const FN_ANGLE_RAD: &str = "anglerad";
const FN_ANGLE_DEG: &str = "angledeg";

/// Functions with explicit complex-expansion rules below, sorted for the
/// rejection message (Java sorts its set when printing).
const SUPPORTED_FUNCTIONS: [&str; 14] = [
    "abs",
    FN_ANGLE,
    FN_ANGLE_DEG,
    FN_ANGLE_RAD,
    "cis",
    "conj",
    "cos",
    "exp",
    "imag",
    "ln",
    FN_MAGNITUDE,
    "real",
    "sin",
    "sqrt",
];

/// Expand complex arithmetic into real scalar equations.
///
/// * `complex_mode` **on**: every equation is split into a `… (real)` and
///   (unless identically `0 = 0`) a `… (imag)` equation over `_r`/`_i`
///   variables, and undetermined imaginary parts are pinned to zero — the
///   full Java `ComplexExpansion.expand`.
/// * `complex_mode` **off**: an imaginary literal anywhere is the Java
///   `requireComplexModeForImaginaryLiterals` hard error; otherwise the
///   equations pass through **unchanged** (the golden corpus freezes the real
///   pipeline).
pub fn expand_complex(equations: Vec<Equation>, complex_mode: bool) -> Result<Vec<Equation>> {
    if !complex_mode {
        if mentions_imaginary(&equations) {
            return Err(FreesError::solver(
                "The equations contain complex literals (e.g. 1i): \
                 enable Complex mode to solve them.",
            ));
        }
        return Ok(equations);
    }
    let mut display_names = HashMap::new();
    expand_with_display_names(&equations, &mut display_names)
}

/// The full Java `ComplexExpansion.expand(equations, displayNames)`:
/// expansion **plus** display-name propagation. For every variable `x` of
/// every equation, `x_r`/`x_i` display as `<display of x>_r` / `_i`
/// (`displayNames.getOrDefault(varName, varName)` + suffix).
///
/// Runs unconditionally — the caller decides complex mode is on. Use
/// [`expand_complex`] for the mode-gated entry point.
pub fn expand_with_display_names(
    equations: &[Equation],
    display_names: &mut HashMap<String, String>,
) -> Result<Vec<Equation>> {
    let mut expanded = Vec::new();
    let mut base_vars: BTreeSet<String> = BTreeSet::new();

    generate_real_imag_equations(equations, display_names, &mut base_vars, &mut expanded)?;

    // Collect all expanded variables and their real/imag parts.
    let mut all_vars: BTreeSet<String> = BTreeSet::new();
    for base_var in &base_vars {
        all_vars.insert(format!("{base_var}_r"));
        all_vars.insert(format!("{base_var}_i"));
    }

    // Run bipartite matching to find unmatched variables.
    let (mut var_to_eq, mut eq_to_var) = match_expanded_variables(&expanded, &all_vars);

    pin_unmatched_imaginary_parts(
        &mut expanded,
        &all_vars,
        &mut var_to_eq,
        &mut eq_to_var,
        equations,
    );

    Ok(expanded)
}

/// Whether any equation contains an imaginary literal (`1i`, `2j`, …).
/// Java `ComplexExpansion.mentionsImaginary(List<Equation>)`.
pub fn mentions_imaginary(equations: &[Equation]) -> bool {
    equations
        .iter()
        .any(|eq| expr_mentions_imaginary(&eq.lhs) || expr_mentions_imaginary(&eq.rhs))
}

fn expr_mentions_imaginary(e: &Expr) -> bool {
    match e {
        Expr::Num { is_imaginary, .. } => *is_imaginary,
        Expr::Str(_) | Expr::Var(_) => false,
        Expr::Neg(operand) | Expr::Not(operand) => expr_mentions_imaginary(operand),
        Expr::BinOp { left, right, .. }
        | Expr::Compare { left, right, .. }
        | Expr::Logical { left, right, .. }
        | Expr::Range {
            start: left,
            end: right,
        } => expr_mentions_imaginary(left) || expr_mentions_imaginary(right),
        Expr::Call { args, .. } => args.iter().any(expr_mentions_imaginary),
        Expr::ArrayAccess { indices, .. } => indices.iter().any(expr_mentions_imaginary),
        Expr::ArrayLiteral(elements) => elements.iter().any(expr_mentions_imaginary),
    }
}

// ---------------------------------------------------------------------------
// Equation generation
// ---------------------------------------------------------------------------

fn generate_real_imag_equations(
    equations: &[Equation],
    display_names: &mut HashMap<String, String>,
    base_vars: &mut BTreeSet<String>,
    expanded: &mut Vec<Equation>,
) -> Result<()> {
    for eq in equations {
        base_vars.extend(eq.variables());

        let lr = simplify(real_part(&eq.lhs)?);
        let rr = simplify(real_part(&eq.rhs)?);
        expanded.push(Equation::new(lr, rr, format!("{} (real)", eq.source_text)));

        let li = simplify(imag_part(&eq.lhs)?);
        let ri = simplify(imag_part(&eq.rhs)?);
        if !(is_literal_zero(&li) && is_literal_zero(&ri)) {
            expanded.push(Equation::new(li, ri, format!("{} (imag)", eq.source_text)));
        }

        for var_name in eq.variables() {
            let disp = display_names
                .get(&var_name)
                .cloned()
                .unwrap_or_else(|| var_name.clone());
            display_names.insert(format!("{var_name}_r"), format!("{disp}_r"));
            display_names.insert(format!("{var_name}_i"), format!("{disp}_i"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Real / imaginary part extraction
// ---------------------------------------------------------------------------

/// The real part of `e` over `_r`/`_i` scalar variables.
/// Java `ComplexExpansion.realPart`.
pub fn real_part(e: &Expr) -> Result<Expr> {
    Ok(match e {
        Expr::Num {
            value: _,
            unit,
            is_imaginary,
        } => {
            if *is_imaginary {
                Expr::Num {
                    value: 0.0,
                    unit: unit.clone(),
                    is_imaginary: false,
                }
            } else {
                e.clone()
            }
        }
        Expr::Str(_) => e.clone(),
        Expr::Var(name) => Expr::Var(format!("{name}_r")),
        Expr::Neg(operand) => Expr::Neg(Box::new(real_part(operand)?)),
        Expr::BinOp { op, left, right } => {
            let lr = real_part(left)?;
            let li = imag_part(left)?;
            let rr = real_part(right)?;
            let ri = imag_part(right)?;
            match op {
                BinOp::Add => Expr::bin(BinOp::Add, lr, rr),
                BinOp::Sub => Expr::bin(BinOp::Sub, lr, rr),
                BinOp::Mul => Expr::bin(
                    BinOp::Sub,
                    Expr::bin(BinOp::Mul, lr, rr),
                    Expr::bin(BinOp::Mul, li, ri),
                ),
                BinOp::Div => {
                    let denom = Expr::bin(
                        BinOp::Add,
                        Expr::bin(BinOp::Mul, rr.clone(), rr.clone()),
                        Expr::bin(BinOp::Mul, ri.clone(), ri.clone()),
                    );
                    let num = Expr::bin(
                        BinOp::Add,
                        Expr::bin(BinOp::Mul, lr, rr),
                        Expr::bin(BinOp::Mul, li, ri),
                    );
                    Expr::bin(BinOp::Div, num, denom)
                }
                BinOp::Pow => {
                    let magnitude = power_magnitude(&lr, &li, &rr, &ri);
                    let angle = power_angle(&lr, &li, &rr, &ri);
                    Expr::bin(BinOp::Mul, magnitude, Expr::call("cos", vec![angle]))
                }
                other => return Err(unknown_operator(*other)),
            }
        }
        Expr::Call { function, args } => real_part_call(function, args)?,
        Expr::ArrayAccess { name, indices } => Expr::ArrayAccess {
            name: format!("{name}_r"),
            indices: indices.clone(),
        },
        Expr::Range { start, end } => Expr::Range {
            start: Box::new(real_part(start)?),
            end: Box::new(real_part(end)?),
        },
        Expr::ArrayLiteral(elements) => {
            Expr::ArrayLiteral(elements.iter().map(real_part).collect::<Result<Vec<_>>>()?)
        }
        // Comparisons and logical ops are always real (evaluate to 1.0 or 0.0).
        Expr::Compare { op, left, right } => Expr::Compare {
            op: *op,
            left: Box::new(real_part(left)?),
            right: Box::new(real_part(right)?),
        },
        Expr::Logical { op, left, right } => Expr::Logical {
            op: *op,
            left: Box::new(real_part(left)?),
            right: Box::new(real_part(right)?),
        },
        Expr::Not(operand) => Expr::Not(Box::new(real_part(operand)?)),
    })
}

/// The imaginary part of `e` over `_r`/`_i` scalar variables.
/// Java `ComplexExpansion.imagPart`.
pub fn imag_part(e: &Expr) -> Result<Expr> {
    Ok(match e {
        Expr::Num {
            value,
            unit,
            is_imaginary,
        } => {
            if *is_imaginary {
                Expr::Num {
                    value: *value,
                    unit: unit.clone(),
                    is_imaginary: false,
                }
            } else {
                Expr::num(0.0)
            }
        }
        Expr::Str(_) => Expr::num(0.0),
        Expr::Var(name) => Expr::Var(format!("{name}_i")),
        Expr::Neg(operand) => Expr::Neg(Box::new(imag_part(operand)?)),
        Expr::BinOp { op, left, right } => {
            let lr = real_part(left)?;
            let li = imag_part(left)?;
            let rr = real_part(right)?;
            let ri = imag_part(right)?;
            match op {
                BinOp::Add => Expr::bin(BinOp::Add, li, ri),
                BinOp::Sub => Expr::bin(BinOp::Sub, li, ri),
                BinOp::Mul => Expr::bin(
                    BinOp::Add,
                    Expr::bin(BinOp::Mul, lr, ri),
                    Expr::bin(BinOp::Mul, li, rr),
                ),
                BinOp::Div => {
                    let denom = Expr::bin(
                        BinOp::Add,
                        Expr::bin(BinOp::Mul, rr.clone(), rr.clone()),
                        Expr::bin(BinOp::Mul, ri.clone(), ri.clone()),
                    );
                    let num = Expr::bin(
                        BinOp::Sub,
                        Expr::bin(BinOp::Mul, li, rr),
                        Expr::bin(BinOp::Mul, lr, ri),
                    );
                    Expr::bin(BinOp::Div, num, denom)
                }
                BinOp::Pow => {
                    let magnitude = power_magnitude(&lr, &li, &rr, &ri);
                    let angle = power_angle(&lr, &li, &rr, &ri);
                    Expr::bin(BinOp::Mul, magnitude, Expr::call("sin", vec![angle]))
                }
                other => return Err(unknown_operator(*other)),
            }
        }
        Expr::Call { function, args } => imag_part_call(function, args)?,
        Expr::ArrayAccess { name, indices } => Expr::ArrayAccess {
            name: format!("{name}_i"),
            indices: indices.clone(),
        },
        Expr::Range { start, end } => Expr::Range {
            start: Box::new(imag_part(start)?),
            end: Box::new(imag_part(end)?),
        },
        Expr::ArrayLiteral(elements) => {
            Expr::ArrayLiteral(elements.iter().map(imag_part).collect::<Result<Vec<_>>>()?)
        }
        // Comparisons and logical ops have zero imaginary part.
        Expr::Compare { .. } | Expr::Logical { .. } | Expr::Not(_) => Expr::num(0.0),
    })
}

fn real_part_call(function: &str, args: &[Expr]) -> Result<Expr> {
    match function {
        "real" => real_part(first_arg(function, args)?),
        "imag" => imag_part(first_arg(function, args)?),
        "sin" => {
            let arg = first_arg(function, args)?;
            let x = real_part(arg)?;
            let y = imag_part(arg)?;
            Ok(Expr::bin(
                BinOp::Mul,
                Expr::call("sin", vec![x]),
                cosh_of(&y),
            ))
        }
        "cos" => {
            let arg = first_arg(function, args)?;
            let x = real_part(arg)?;
            let y = imag_part(arg)?;
            Ok(Expr::bin(
                BinOp::Mul,
                Expr::call("cos", vec![x]),
                cosh_of(&y),
            ))
        }
        "exp" => {
            let arg = first_arg(function, args)?;
            let x = real_part(arg)?;
            let y = imag_part(arg)?;
            let exp_x = Expr::call("exp", vec![x]);
            Ok(Expr::bin(BinOp::Mul, exp_x, Expr::call("cos", vec![y])))
        }
        "ln" => {
            let arg = first_arg(function, args)?;
            let x = real_part(arg)?;
            let y = imag_part(arg)?;
            let r2 = squared_modulus(&x, &y);
            Ok(Expr::bin(
                BinOp::Mul,
                Expr::num(0.5),
                Expr::call("ln", vec![r2]),
            ))
        }
        "sqrt" => {
            let arg = first_arg(function, args)?;
            let x = real_part(arg)?;
            let y = imag_part(arg)?;
            let r = Expr::call("sqrt", vec![squared_modulus(&x, &y)]);
            let theta = Expr::call(ATAN2, vec![y, x]);
            let sqrt_r = Expr::call("sqrt", vec![r]);
            let half_theta = Expr::bin(BinOp::Mul, Expr::num(0.5), theta);
            Ok(Expr::bin(
                BinOp::Mul,
                sqrt_r,
                Expr::call("cos", vec![half_theta]),
            ))
        }
        // |z| = sqrt(x^2 + y^2): the complex magnitude.
        "abs" | FN_MAGNITUDE => {
            let arg = first_arg(function, args)?;
            let x = real_part(arg)?;
            let y = imag_part(arg)?;
            Ok(Expr::call("sqrt", vec![squared_modulus(&x, &y)]))
        }
        "conj" => real_part(first_arg(function, args)?),
        FN_ANGLE | FN_ANGLE_RAD => {
            let arg = first_arg(function, args)?;
            let x = real_part(arg)?;
            let y = imag_part(arg)?;
            Ok(Expr::call(ATAN2, vec![y, x]))
        }
        FN_ANGLE_DEG => {
            let arg = first_arg(function, args)?;
            let x = real_part(arg)?;
            let y = imag_part(arg)?;
            let rad = Expr::call(ATAN2, vec![y, x]);
            Ok(Expr::bin(
                BinOp::Mul,
                rad,
                Expr::bin(
                    BinOp::Div,
                    Expr::num(180.0),
                    Expr::num(std::f64::consts::PI),
                ),
            ))
        }
        "cis" => {
            let theta = real_part(first_arg(function, args)?)?;
            Ok(Expr::call("cos", vec![theta]))
        }
        _ => Err(unsupported_in_complex_mode(function)),
    }
}

fn imag_part_call(function: &str, args: &[Expr]) -> Result<Expr> {
    match function {
        "real" | "imag" => {
            first_arg(function, args)?;
            Ok(Expr::num(0.0))
        }
        "sin" => {
            let arg = first_arg(function, args)?;
            let x = real_part(arg)?;
            let y = imag_part(arg)?;
            Ok(Expr::bin(
                BinOp::Mul,
                Expr::call("cos", vec![x]),
                sinh_of(&y),
            ))
        }
        "cos" => {
            let arg = first_arg(function, args)?;
            let x = real_part(arg)?;
            let y = imag_part(arg)?;
            Ok(Expr::Neg(Box::new(Expr::bin(
                BinOp::Mul,
                Expr::call("sin", vec![x]),
                sinh_of(&y),
            ))))
        }
        "exp" => {
            let arg = first_arg(function, args)?;
            let x = real_part(arg)?;
            let y = imag_part(arg)?;
            let exp_x = Expr::call("exp", vec![x]);
            Ok(Expr::bin(BinOp::Mul, exp_x, Expr::call("sin", vec![y])))
        }
        "ln" => {
            let arg = first_arg(function, args)?;
            let x = real_part(arg)?;
            let y = imag_part(arg)?;
            Ok(Expr::call(ATAN2, vec![y, x]))
        }
        "sqrt" => {
            let arg = first_arg(function, args)?;
            let x = real_part(arg)?;
            let y = imag_part(arg)?;
            let r = Expr::call("sqrt", vec![squared_modulus(&x, &y)]);
            let theta = Expr::call(ATAN2, vec![y, x]);
            let sqrt_r = Expr::call("sqrt", vec![r]);
            let half_theta = Expr::bin(BinOp::Mul, Expr::num(0.5), theta);
            Ok(Expr::bin(
                BinOp::Mul,
                sqrt_r,
                Expr::call("sin", vec![half_theta]),
            ))
        }
        // The magnitude (and the angle functions) are real; their imaginary
        // part is zero.
        "abs" | FN_MAGNITUDE | FN_ANGLE | FN_ANGLE_RAD | FN_ANGLE_DEG => {
            first_arg(function, args)?;
            Ok(Expr::num(0.0))
        }
        "conj" => Ok(Expr::Neg(Box::new(imag_part(first_arg(function, args)?)?))),
        "cis" => {
            let theta = real_part(first_arg(function, args)?)?;
            Ok(Expr::call("sin", vec![theta]))
        }
        _ => Err(unsupported_in_complex_mode(function)),
    }
}

/// `|z^w|` for `z = a+bi`, `w = c+di`: `r^c * e^(-d*theta)`. When `d` is the
/// literal zero (real exponent, the common case) the `e`-term is omitted so
/// that `z = 0` does not produce `ln(0)*0 = NaN` during iteration.
fn power_magnitude(a: &Expr, b: &Expr, c: &Expr, d: &Expr) -> Expr {
    let r = Expr::call("sqrt", vec![squared_modulus(a, b)]);
    let r_pow_c = Expr::bin(BinOp::Pow, r, c.clone());
    if is_literal_zero(d) {
        return r_pow_c;
    }
    let theta = Expr::call(ATAN2, vec![b.clone(), a.clone()]);
    let e_term = Expr::call(
        "exp",
        vec![Expr::Neg(Box::new(Expr::bin(BinOp::Mul, d.clone(), theta)))],
    );
    Expr::bin(BinOp::Mul, r_pow_c, e_term)
}

/// `arg(z^w)`: `c*theta + d*ln(r)`, with the `d`-term omitted for real
/// exponents.
fn power_angle(a: &Expr, b: &Expr, c: &Expr, d: &Expr) -> Expr {
    let theta = Expr::call(ATAN2, vec![b.clone(), a.clone()]);
    let c_theta = Expr::bin(BinOp::Mul, c.clone(), theta);
    if is_literal_zero(d) {
        return c_theta;
    }
    let ln_r = Expr::bin(
        BinOp::Mul,
        Expr::num(0.5),
        Expr::call("ln", vec![squared_modulus(a, b)]),
    );
    Expr::bin(BinOp::Add, c_theta, Expr::bin(BinOp::Mul, d.clone(), ln_r))
}

/// `a*a + b*b` — the squared modulus subtree every polar rule shares.
fn squared_modulus(a: &Expr, b: &Expr) -> Expr {
    Expr::bin(
        BinOp::Add,
        Expr::bin(BinOp::Mul, a.clone(), a.clone()),
        Expr::bin(BinOp::Mul, b.clone(), b.clone()),
    )
}

/// `cosh(y)` spelled `0.5 * (exp(y) + exp(-y))` exactly as the Java rules
/// build it (the engine has no `cosh` requirement this way).
fn cosh_of(y: &Expr) -> Expr {
    let exp_y = Expr::call("exp", vec![y.clone()]);
    let exp_neg_y = Expr::call("exp", vec![Expr::Neg(Box::new(y.clone()))]);
    Expr::bin(
        BinOp::Mul,
        Expr::num(0.5),
        Expr::bin(BinOp::Add, exp_y, exp_neg_y),
    )
}

/// `sinh(y)` spelled `0.5 * (exp(y) - exp(-y))`.
fn sinh_of(y: &Expr) -> Expr {
    let exp_y = Expr::call("exp", vec![y.clone()]);
    let exp_neg_y = Expr::call("exp", vec![Expr::Neg(Box::new(y.clone()))]);
    Expr::bin(
        BinOp::Mul,
        Expr::num(0.5),
        Expr::bin(BinOp::Sub, exp_y, exp_neg_y),
    )
}

fn unsupported_in_complex_mode(function: &str) -> FreesError {
    // Silently mapping real/imag parts through an arbitrary function is
    // mathematically wrong (Im tan(z) != tan(Im z)); reject instead.
    FreesError::parse(format!(
        "Function '{function}' is not supported in complex mode. Supported: {}",
        SUPPORTED_FUNCTIONS.join(", ")
    ))
}

fn unknown_operator(op: BinOp) -> FreesError {
    // Java hits `default -> throw new IllegalStateException("Unknown
    // operator: " + op)` here; a typed refusal beats a 500.
    FreesError::parse(format!(
        "Operator '{}' is not supported in complex mode.",
        op.as_str()
    ))
}

fn first_arg<'a>(function: &str, args: &'a [Expr]) -> Result<&'a Expr> {
    // Java reads `args.get(0)` (extra arguments are ignored; an empty list is
    // an IndexOutOfBoundsException). Keep the lenient arity, fail cleanly.
    args.first().ok_or_else(|| {
        FreesError::evaluation(format!(
            "Function '{function}' expects an argument in complex mode."
        ))
    })
}

// ---------------------------------------------------------------------------
// Simplification
// ---------------------------------------------------------------------------

fn is_literal_zero(e: &Expr) -> bool {
    // Java has `isLiteralZero` and `isZeroNum` with identical semantics:
    // any Num whose value is 0.0, unit and imaginary flag ignored.
    matches!(e, Expr::Num { value, .. } if *value == 0.0)
}

fn is_one_num(e: &Expr) -> bool {
    matches!(e, Expr::Num { value, .. } if *value == 1.0)
}

/// Whether the expression is non-negative by construction.
fn is_non_negative(e: &Expr) -> bool {
    match e {
        Expr::Num {
            value,
            is_imaginary,
            ..
        } => !*is_imaginary && *value >= 0.0,
        Expr::Call { function, .. } => function == "sqrt" || function == "abs",
        _ => false,
    }
}

fn simplify(e: Expr) -> Expr {
    match e {
        Expr::Neg(operand) => {
            let op = simplify(*operand);
            if is_literal_zero(&op) {
                return op;
            }
            if let Expr::Neg(inner) = op {
                return *inner;
            }
            Expr::Neg(Box::new(op))
        }
        Expr::BinOp { op, left, right } => simplify_bin_op(op, *left, *right),
        Expr::Call { function, args } => {
            let simplified_args: Vec<Expr> = args.into_iter().map(simplify).collect();
            // The expansion rules wrap structurally real subtrees in
            // sin/cos/atan2 (e.g. |z|^w produces sin(w*atan2(0, sqrt(..)))).
            // Folding these reveals which imaginary parts are identically
            // zero, so the matching below sees the true structure.
            if function == ATAN2
                && simplified_args.len() == 2
                && is_literal_zero(&simplified_args[0])
                && is_non_negative(&simplified_args[1])
            {
                return Expr::num(0.0);
            }
            if function == "sin"
                && simplified_args.len() == 1
                && is_literal_zero(&simplified_args[0])
            {
                return Expr::num(0.0);
            }
            if function == "cos"
                && simplified_args.len() == 1
                && is_literal_zero(&simplified_args[0])
            {
                return Expr::num(1.0);
            }
            Expr::Call {
                function,
                args: simplified_args,
            }
        }
        other => other,
    }
}

fn simplify_bin_op(op: BinOp, left: Expr, right: Expr) -> Expr {
    let l = simplify(left);
    let r = simplify(right);
    let l_zero = is_literal_zero(&l);
    let r_zero = is_literal_zero(&r);
    let l_one = is_one_num(&l);
    let r_one = is_one_num(&r);

    match op {
        BinOp::Add => {
            if l_zero {
                r
            } else if r_zero {
                l
            } else {
                Expr::bin(op, l, r)
            }
        }
        BinOp::Sub => {
            if r_zero {
                l
            } else if l_zero {
                Expr::Neg(Box::new(r))
            } else {
                Expr::bin(op, l, r)
            }
        }
        BinOp::Mul => {
            if l_zero || r_zero {
                Expr::num(0.0)
            } else if l_one {
                r
            } else if r_one {
                l
            } else {
                Expr::bin(op, l, r)
            }
        }
        BinOp::Div => {
            if l_zero {
                Expr::num(0.0)
            } else if r_one {
                l
            } else {
                Expr::bin(op, l, r)
            }
        }
        BinOp::Pow => {
            if r_zero {
                Expr::num(1.0)
            } else if r_one {
                l
            } else if l_zero {
                Expr::num(0.0)
            } else {
                Expr::bin(op, l, r)
            }
        }
        other => Expr::bin(other, l, r),
    }
}

// ---------------------------------------------------------------------------
// Matching and imaginary-part pinning
// ---------------------------------------------------------------------------

/// Maximum bipartite matching between expanded equations and `all_vars`,
/// returned as the Java pair of maps (`varToEq`, `eqToVar`).
///
/// Java runs JGraphT's Hopcroft–Karp; the augmenting-path matching here has
/// the same cardinality, and the pinning step below canonicalises which `_i`
/// variable ends up pinned, so the observable result matches.
fn match_expanded_variables(
    expanded: &[Equation],
    all_vars: &BTreeSet<String>,
) -> (HashMap<String, usize>, HashMap<usize, String>) {
    let var_names: Vec<&String> = all_vars.iter().collect();
    let var_index: HashMap<&str, usize> = var_names
        .iter()
        .enumerate()
        .map(|(i, v)| (v.as_str(), i))
        .collect();
    // Variables outside `all_vars` (bare array indices) take no part.
    let adjacency: Vec<Vec<usize>> = expanded
        .iter()
        .map(|eq| {
            eq.variables()
                .iter()
                .filter_map(|v| var_index.get(v.as_str()).copied())
                .collect()
        })
        .collect();

    let mut eq_of_var: Vec<Option<usize>> = vec![None; var_names.len()];
    let mut var_of_eq: Vec<Option<usize>> = vec![None; expanded.len()];
    for start in 0..expanded.len() {
        augment_from(start, &adjacency, &mut eq_of_var, &mut var_of_eq);
    }

    let mut var_to_eq = HashMap::new();
    let mut eq_to_var = HashMap::new();
    for (v, eq) in eq_of_var.iter().enumerate() {
        if let Some(eq) = eq {
            var_to_eq.insert(var_names[v].clone(), *eq);
            eq_to_var.insert(*eq, var_names[v].clone());
        }
    }
    (var_to_eq, eq_to_var)
}

/// One BFS augmenting-path search from a free equation. Returns whether the
/// matching grew.
fn augment_from(
    start_eq: usize,
    adjacency: &[Vec<usize>],
    eq_of_var: &mut [Option<usize>],
    var_of_eq: &mut [Option<usize>],
) -> bool {
    let mut discovered_via: Vec<Option<usize>> = vec![None; eq_of_var.len()];
    let mut enqueued = vec![false; adjacency.len()];
    let mut queue = VecDeque::new();
    queue.push_back(start_eq);
    enqueued[start_eq] = true;

    while let Some(eq) = queue.pop_front() {
        for &v in &adjacency[eq] {
            if discovered_via[v].is_some() {
                continue;
            }
            discovered_via[v] = Some(eq);
            match eq_of_var[v] {
                None => {
                    // Free variable: flip the alternating path back to start.
                    let mut var = v;
                    loop {
                        let via = discovered_via[var].expect("path recorded");
                        let previous = var_of_eq[via];
                        eq_of_var[var] = Some(via);
                        var_of_eq[via] = Some(var);
                        match previous {
                            Some(prev_var) => var = prev_var,
                            None => return true,
                        }
                    }
                }
                Some(next_eq) => {
                    if !enqueued[next_eq] {
                        enqueued[next_eq] = true;
                        queue.push_back(next_eq);
                    }
                }
            }
        }
    }
    false
}

/// Occurrence count of each base variable over the original equations.
fn count_occurrences(equations: &[Equation]) -> HashMap<String, usize> {
    let mut base_occurrences = HashMap::new();
    for eq in equations {
        for v in eq.variables() {
            *base_occurrences.entry(v).or_insert(0) += 1;
        }
    }
    base_occurrences
}

fn build_eqs_by_var(expanded: &[Equation]) -> HashMap<String, Vec<usize>> {
    let mut eqs_by_var: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, eq) in expanded.iter().enumerate() {
        for var_name in eq.variables() {
            eqs_by_var.entry(var_name).or_default().push(i);
        }
    }
    eqs_by_var
}

/// Every variable the matching left exposed gets its imaginary part pinned to
/// zero: swap the exposure onto the preferred `_i` variable along an
/// alternating path, then add `<var>_i = 0 (default complex real)`. If the
/// system then carries more equations than variables, drop unmatched `(imag)`
/// equations from the back.
fn pin_unmatched_imaginary_parts(
    expanded: &mut Vec<Equation>,
    all_vars: &BTreeSet<String>,
    var_to_eq: &mut HashMap<String, usize>,
    eq_to_var: &mut HashMap<usize, String>,
    equations: &[Equation],
) {
    let base_occurrences = count_occurrences(equations);
    let eqs_by_var = build_eqs_by_var(expanded);
    let size_before_pins = expanded.len();
    for var_name in all_vars {
        if !var_to_eq.contains_key(var_name) {
            let pin = swap_to_preferred_imaginary(
                var_name,
                &eqs_by_var,
                var_to_eq,
                eq_to_var,
                &base_occurrences,
            );
            if let Some(pin) = pin {
                expanded.push(Equation::new(
                    Expr::Var(pin.clone()),
                    Expr::num(0.0),
                    format!("{pin} = 0 (default complex real)"),
                ));
            }
        }
    }

    let excess = expanded.len() as i64 - all_vars.len() as i64;
    if excess > 0 {
        trim_excess_equations(expanded, eq_to_var, size_before_pins, excess as usize);
    }
}

fn trim_excess_equations(
    expanded: &mut Vec<Equation>,
    eq_to_var: &HashMap<usize, String>,
    size_before_pins: usize,
    excess: usize,
) {
    let mut ext = excess;
    let mut i = size_before_pins;
    while i > 0 && ext > 0 {
        i -= 1;
        if !eq_to_var.contains_key(&i) && expanded[i].source_text.ends_with("(imag)") {
            expanded.remove(i);
            ext -= 1;
        }
    }
}

fn flip_alternating_path(
    best: &str,
    exposed_var: &str,
    var_to_eq: &mut HashMap<String, usize>,
    eq_to_var: &mut HashMap<usize, String>,
    reached_via_eq: &HashMap<String, usize>,
    reached_from_var: &HashMap<usize, String>,
) {
    let mut cur = best.to_string();
    var_to_eq.remove(best);
    while cur != exposed_var {
        let eq = reached_via_eq[cur.as_str()];
        let prev = reached_from_var[&eq].clone();
        eq_to_var.insert(eq, prev.clone());
        var_to_eq.insert(prev.clone(), eq);
        cur = prev;
    }
}

/// BFS over alternating paths from `exposed_var`; among the reachable `_i`
/// variables (including itself), pick the one whose **base** variable occurs
/// in the fewest original equations (ties broken lexicographically), flip the
/// path so it becomes the exposed one, and return it as the pin target.
/// `None` when no `_i` variable is reachable.
fn swap_to_preferred_imaginary(
    exposed_var: &str,
    eqs_by_var: &HashMap<String, Vec<usize>>,
    var_to_eq: &mut HashMap<String, usize>,
    eq_to_var: &mut HashMap<usize, String>,
    base_occurrences: &HashMap<String, usize>,
) -> Option<String> {
    let mut reached_via_eq: HashMap<String, usize> = HashMap::new();
    let mut reached_from_var: HashMap<usize, String> = HashMap::new();
    let mut seen_vars: HashSet<String> = HashSet::new();
    let mut seen_eqs: HashSet<usize> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(exposed_var.to_string());
    seen_vars.insert(exposed_var.to_string());

    let mut candidates: Vec<String> = Vec::new();
    if exposed_var.ends_with("_i") {
        candidates.push(exposed_var.to_string());
    }
    while let Some(v) = queue.pop_front() {
        let Some(eq_indices) = eqs_by_var.get(&v) else {
            continue;
        };
        for &ei in eq_indices {
            if seen_eqs.insert(ei) {
                reached_from_var.insert(ei, v.clone());
                if let Some(w) = eq_to_var.get(&ei).cloned() {
                    if seen_vars.insert(w.clone()) {
                        reached_via_eq.insert(w.clone(), ei);
                        if w.ends_with("_i") {
                            candidates.push(w.clone());
                        }
                        queue.push_back(w);
                    }
                }
            }
        }
    }
    let occurrences_of = |w: &str| -> usize {
        base_occurrences
            .get(&w[..w.len() - 2])
            .copied()
            .unwrap_or(0)
    };
    let best = candidates
        .into_iter()
        .min_by(|a, b| occurrences_of(a).cmp(&occurrences_of(b)).then(a.cmp(b)))?;
    flip_alternating_path(
        &best,
        exposed_var,
        var_to_eq,
        eq_to_var,
        &reached_via_eq,
        &reached_from_var,
    );
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_document;

    fn parse_equations(source: &str) -> Vec<Equation> {
        parse_document(source)
            .expect("parse")
            .equations()
            .into_iter()
            .cloned()
            .collect()
    }

    fn expand_source(source: &str) -> Vec<Equation> {
        expand_complex(parse_equations(source), true).expect("expand")
    }

    fn imaginary(value: f64) -> Expr {
        Expr::Num {
            value,
            unit: None,
            is_imaginary: true,
        }
    }

    // ── trigger semantics ───────────────────────────────────────────────────

    #[test]
    fn real_mode_passes_purely_real_systems_through_unchanged() {
        let equations = parse_equations("x^2 + y^3 = 77\nx / y = 1.23456");
        let expanded = expand_complex(equations.clone(), false).unwrap();
        assert_eq!(expanded, equations);
    }

    #[test]
    fn real_mode_leaves_real_and_imag_calls_alone() {
        // `realModeRealAndImag` upstream: in real mode the functions stay as
        // calls for the evaluator; the expansion must not touch them.
        let equations = parse_equations("x = 5\na = real(x)\nb = imag(x)");
        let expanded = expand_complex(equations.clone(), false).unwrap();
        assert_eq!(expanded, equations);
    }

    #[test]
    fn real_mode_rejects_imaginary_literals_with_the_java_message() {
        let equations = parse_equations("z = 3 + 4i");
        let err = expand_complex(equations, false).unwrap_err();
        assert_eq!(
            err,
            FreesError::solver(
                "The equations contain complex literals (e.g. 1i): \
                 enable Complex mode to solve them."
            )
        );
    }

    #[test]
    fn mentions_imaginary_looks_inside_every_variant() {
        assert!(mentions_imaginary(&parse_equations("z = 1i")));
        assert!(mentions_imaginary(&parse_equations("z = -(2 * 3j)")));
        assert!(mentions_imaginary(&parse_equations("z = abs(1 + 2i)")));
        assert!(!mentions_imaginary(&parse_equations("z = abs(1 + 2)")));
        assert!(!mentions_imaginary(&parse_equations("x = 5\ny = x^2")));
    }

    // ── variable and literal splitting ──────────────────────────────────────

    #[test]
    fn splits_a_complex_literal_assignment_into_real_and_imag_equations() {
        let expanded = expand_source("z = 3 + 4i");
        assert_eq!(expanded.len(), 2);

        assert_eq!(expanded[0].source_text, "z = 3 + 4i (real)");
        assert_eq!(expanded[0].lhs, Expr::var("z_r"));
        // realPart(3 + 4i) = 3 + 0, simplified to 3.
        assert_eq!(expanded[0].rhs, Expr::num(3.0));

        assert_eq!(expanded[1].source_text, "z = 3 + 4i (imag)");
        assert_eq!(expanded[1].lhs, Expr::var("z_i"));
        // imagPart(3 + 4i) = 0 + 4, simplified to 4 (imaginary flag dropped).
        assert_eq!(expanded[1].rhs, Expr::num(4.0));
    }

    #[test]
    fn display_names_gain_suffixed_entries() {
        let equations = parse_equations("z = 3 + 4i");
        let mut names = HashMap::from([("z".to_string(), "Z".to_string())]);
        expand_with_display_names(&equations, &mut names).unwrap();
        assert_eq!(names["z_r"], "Z_r");
        assert_eq!(names["z_i"], "Z_i");
        // Unmapped variables display as themselves plus the suffix.
        let mut bare = HashMap::new();
        expand_with_display_names(&equations, &mut bare).unwrap();
        assert_eq!(bare["z_r"], "z_r");
        assert_eq!(bare["z_i"], "z_i");
    }

    #[test]
    fn real_part_of_an_imaginary_literal_keeps_its_unit_on_the_zero() {
        let lit = Expr::Num {
            value: 4.0,
            unit: Some("A".into()),
            is_imaginary: true,
        };
        assert_eq!(
            real_part(&lit).unwrap(),
            Expr::Num {
                value: 0.0,
                unit: Some("A".into()),
                is_imaginary: false,
            }
        );
        assert_eq!(
            imag_part(&lit).unwrap(),
            Expr::Num {
                value: 4.0,
                unit: Some("A".into()),
                is_imaginary: false,
            }
        );
        // A real literal's imaginary part is a fresh unit-less zero.
        let real_lit = Expr::Num {
            value: 7.0,
            unit: Some("m".into()),
            is_imaginary: false,
        };
        assert_eq!(imag_part(&real_lit).unwrap(), Expr::num(0.0));
        assert_eq!(real_part(&real_lit).unwrap(), real_lit);
    }

    // ── operator splitting rules ────────────────────────────────────────────

    #[test]
    fn product_splits_by_the_foil_rule() {
        // z * w: real = z_r*w_r - z_i*w_i, imag = z_r*w_i + z_i*w_r.
        let e = Expr::bin(BinOp::Mul, Expr::var("z"), Expr::var("w"));
        assert_eq!(
            real_part(&e).unwrap(),
            Expr::bin(
                BinOp::Sub,
                Expr::bin(BinOp::Mul, Expr::var("z_r"), Expr::var("w_r")),
                Expr::bin(BinOp::Mul, Expr::var("z_i"), Expr::var("w_i")),
            )
        );
        assert_eq!(
            imag_part(&e).unwrap(),
            Expr::bin(
                BinOp::Add,
                Expr::bin(BinOp::Mul, Expr::var("z_r"), Expr::var("w_i")),
                Expr::bin(BinOp::Mul, Expr::var("z_i"), Expr::var("w_r")),
            )
        );
    }

    #[test]
    fn quotient_splits_over_the_conjugate_denominator() {
        let e = Expr::bin(BinOp::Div, Expr::var("z"), Expr::var("w"));
        let denom = Expr::bin(
            BinOp::Add,
            Expr::bin(BinOp::Mul, Expr::var("w_r"), Expr::var("w_r")),
            Expr::bin(BinOp::Mul, Expr::var("w_i"), Expr::var("w_i")),
        );
        assert_eq!(
            real_part(&e).unwrap(),
            Expr::bin(
                BinOp::Div,
                Expr::bin(
                    BinOp::Add,
                    Expr::bin(BinOp::Mul, Expr::var("z_r"), Expr::var("w_r")),
                    Expr::bin(BinOp::Mul, Expr::var("z_i"), Expr::var("w_i")),
                ),
                denom.clone(),
            )
        );
        assert_eq!(
            imag_part(&e).unwrap(),
            Expr::bin(
                BinOp::Div,
                Expr::bin(
                    BinOp::Sub,
                    Expr::bin(BinOp::Mul, Expr::var("z_i"), Expr::var("w_r")),
                    Expr::bin(BinOp::Mul, Expr::var("z_r"), Expr::var("w_i")),
                ),
                denom,
            )
        );
    }

    #[test]
    fn real_exponent_power_omits_the_exp_and_ln_terms() {
        // z^2: |z^2| = sqrt(z_r^2 + z_i^2)^2 — no e^(-d·θ) factor — and
        // arg = 2*atan2(z_i, z_r) — no d·ln r term.
        let e = Expr::bin(BinOp::Pow, Expr::var("z"), Expr::num(2.0));
        let r = Expr::call(
            "sqrt",
            vec![Expr::bin(
                BinOp::Add,
                Expr::bin(BinOp::Mul, Expr::var("z_r"), Expr::var("z_r")),
                Expr::bin(BinOp::Mul, Expr::var("z_i"), Expr::var("z_i")),
            )],
        );
        let magnitude = Expr::bin(BinOp::Pow, r, Expr::num(2.0));
        let angle = Expr::bin(
            BinOp::Mul,
            Expr::num(2.0),
            Expr::call("atan2", vec![Expr::var("z_i"), Expr::var("z_r")]),
        );
        assert_eq!(
            real_part(&e).unwrap(),
            Expr::bin(
                BinOp::Mul,
                magnitude.clone(),
                Expr::call("cos", vec![angle.clone()])
            )
        );
        assert_eq!(
            imag_part(&e).unwrap(),
            Expr::bin(BinOp::Mul, magnitude, Expr::call("sin", vec![angle]))
        );
    }

    #[test]
    fn complex_exponent_power_carries_the_full_de_moivre_terms() {
        let e = Expr::bin(BinOp::Pow, Expr::var("z"), Expr::var("w"));
        let real = real_part(&e).unwrap();
        let printed = format!("{real:?}");
        // The e^(-w_i*θ) magnitude correction and the w_i*ln(r) angle term
        // must both be present once the exponent's imaginary part is not the
        // literal zero.
        assert!(printed.contains("exp"), "missing e-term: {printed}");
        assert!(printed.contains("ln"), "missing ln-term: {printed}");
    }

    #[test]
    fn matrix_operators_are_rejected_not_panicked() {
        for op in [
            BinOp::LeftDiv,
            BinOp::ElemMul,
            BinOp::ElemDiv,
            BinOp::ElemLeftDiv,
            BinOp::ElemPow,
        ] {
            let e = Expr::bin(op, Expr::var("z"), Expr::var("w"));
            let err = real_part(&e).unwrap_err();
            assert_eq!(
                err,
                FreesError::parse(format!(
                    "Operator '{}' is not supported in complex mode.",
                    op.as_str()
                ))
            );
            assert!(imag_part(&e).is_err());
        }
    }

    // ── function rules ──────────────────────────────────────────────────────

    /// Numerically compare the expanded real/imag parts of `f(z)` against a
    /// closed-form complex evaluation at z = 1.3 + 0.7i.
    ///
    /// `crate::eval::eval` is the engine's pure AST interpreter over parsed
    /// `Expr` trees (no code execution of any kind).
    fn check_function_parts(function: &str, expected_re: f64, expected_im: f64) {
        let scope: crate::eval::Scope = [("z_r".to_string(), 1.3), ("z_i".to_string(), 0.7)].into();
        let call = Expr::call(function, vec![Expr::var("z")]);
        let re = crate::eval::eval(&real_part(&call).unwrap(), &scope).unwrap();
        let im = crate::eval::eval(&imag_part(&call).unwrap(), &scope).unwrap();
        assert!(
            (re - expected_re).abs() < 1e-12,
            "Re {function}(z): got {re}, want {expected_re}"
        );
        assert!(
            (im - expected_im).abs() < 1e-12,
            "Im {function}(z): got {im}, want {expected_im}"
        );
    }

    #[test]
    fn elementary_functions_match_closed_form_complex_values() {
        let (x, y) = (1.3f64, 0.7f64);
        check_function_parts("sin", x.sin() * y.cosh(), x.cos() * y.sinh());
        check_function_parts("cos", x.cos() * y.cosh(), -(x.sin() * y.sinh()));
        check_function_parts("exp", x.exp() * y.cos(), x.exp() * y.sin());
        let r2 = x * x + y * y;
        check_function_parts("ln", 0.5 * r2.ln(), y.atan2(x));
        let modulus = r2.sqrt();
        let half_theta = 0.5 * y.atan2(x);
        check_function_parts(
            "sqrt",
            modulus.sqrt() * half_theta.cos(),
            modulus.sqrt() * half_theta.sin(),
        );
        check_function_parts("abs", modulus, 0.0);
        check_function_parts("magnitude", modulus, 0.0);
        check_function_parts("real", x, 0.0);
        check_function_parts("imag", y, 0.0);
        check_function_parts("conj", x, -y);
        check_function_parts("angle", y.atan2(x), 0.0);
        check_function_parts("anglerad", y.atan2(x), 0.0);
        check_function_parts("angledeg", y.atan2(x).to_degrees(), 0.0);
        // cis uses only the real part of its argument.
        check_function_parts("cis", x.cos(), x.sin());
    }

    #[test]
    fn unsupported_function_error_matches_java_wording() {
        let equations = parse_equations("y = tan(z)\nz = 1");
        let err = expand_complex(equations, true).unwrap_err();
        assert_eq!(
            err,
            FreesError::parse(
                "Function 'tan' is not supported in complex mode. Supported: \
                 abs, angle, angledeg, anglerad, cis, conj, cos, exp, imag, \
                 ln, magnitude, real, sin, sqrt"
            )
        );
    }

    #[test]
    fn zero_argument_supported_function_is_a_clean_error() {
        let e = Expr::call("sin", vec![]);
        assert!(real_part(&e).is_err());
        assert!(imag_part(&e).is_err());
    }

    // ── simplification ──────────────────────────────────────────────────────

    #[test]
    fn simplify_folds_the_identities_java_folds() {
        // atan2(0, sqrt(..)) → 0, sin(0) → 0, cos(0) → 1.
        let atan = Expr::call(
            "atan2",
            vec![Expr::num(0.0), Expr::call("sqrt", vec![Expr::var("q")])],
        );
        assert_eq!(simplify(atan), Expr::num(0.0));
        assert_eq!(
            simplify(Expr::call("sin", vec![Expr::num(0.0)])),
            Expr::num(0.0)
        );
        assert_eq!(
            simplify(Expr::call("cos", vec![Expr::num(0.0)])),
            Expr::num(1.0)
        );
        // atan2(0, x) must NOT fold: x could be negative (angle π).
        let atan_var = Expr::call("atan2", vec![Expr::num(0.0), Expr::var("x")]);
        assert_eq!(simplify(atan_var.clone()), atan_var);

        // 0-identities and 1-identities.
        let x = Expr::var("x");
        assert_eq!(
            simplify(Expr::bin(BinOp::Add, Expr::num(0.0), x.clone())),
            x
        );
        assert_eq!(
            simplify(Expr::bin(BinOp::Sub, Expr::num(0.0), x.clone())),
            Expr::Neg(Box::new(x.clone()))
        );
        assert_eq!(
            simplify(Expr::bin(BinOp::Mul, Expr::num(0.0), x.clone())),
            Expr::num(0.0)
        );
        assert_eq!(
            simplify(Expr::bin(BinOp::Mul, Expr::num(1.0), x.clone())),
            x
        );
        assert_eq!(
            simplify(Expr::bin(BinOp::Div, x.clone(), Expr::num(1.0))),
            x
        );
        assert_eq!(
            simplify(Expr::bin(BinOp::Pow, x.clone(), Expr::num(0.0))),
            Expr::num(1.0)
        );
        assert_eq!(
            simplify(Expr::bin(BinOp::Pow, x.clone(), Expr::num(1.0))),
            x
        );
        assert_eq!(
            simplify(Expr::Neg(Box::new(Expr::Neg(Box::new(x.clone()))))),
            x
        );
        assert_eq!(
            simplify(Expr::Neg(Box::new(Expr::num(0.0)))),
            Expr::num(0.0)
        );
    }

    #[test]
    fn an_imaginary_one_counts_as_one_for_simplification_like_java() {
        // Java's isOneNum ignores the imaginary flag; unreachable after
        // expansion (no imaginary literal survives) but pinned for parity.
        let e = Expr::bin(BinOp::Mul, imaginary(1.0), Expr::var("x"));
        assert_eq!(simplify(e), Expr::var("x"));
    }

    // ── whole-document expansion structure ──────────────────────────────────

    #[test]
    fn purely_real_document_in_complex_mode_pins_imaginary_parts() {
        // real(z) = 3: one equation, z_r and z_i both exist, z_i appears in no
        // equation → pinned directly (it is its own preferred candidate).
        let expanded = expand_source("real(z) = 3");
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].source_text, "real(z) = 3 (real)");
        assert_eq!(expanded[0].lhs, Expr::var("z_r"));
        assert_eq!(expanded[0].rhs, Expr::num(3.0));
        assert_eq!(expanded[1].source_text, "z_i = 0 (default complex real)");
        assert_eq!(expanded[1].lhs, Expr::var("z_i"));
        assert_eq!(expanded[1].rhs, Expr::num(0.0));
    }

    #[test]
    fn abs_equation_drops_its_imag_part_and_pins_via_a_swap() {
        // abs(z) = 5: the imag expansion is 0 = 0 (dropped). The one real
        // equation is matched to z_i first (sorted order), so pinning must
        // swap the exposure from z_r onto z_i along the alternating path.
        let expanded = expand_source("abs(z) = 5");
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].source_text, "abs(z) = 5 (real)");
        assert_eq!(expanded[1].source_text, "z_i = 0 (default complex real)");
    }

    #[test]
    fn pin_prefers_the_imaginary_part_with_fewest_base_occurrences() {
        // abs(z) = abs(w) (imag dropped) plus z + w = 6 (both kept):
        // 3 equations over {z_r, z_i, w_r, w_i} → one pin. z and w tie on
        // occurrences (2 each), so the lexicographically smaller w_i wins.
        let expanded = expand_source("abs(z) = abs(w)\nz + w = 6");
        assert_eq!(expanded.len(), 4);
        assert_eq!(expanded[3].source_text, "w_i = 0 (default complex real)");
    }

    #[test]
    fn overdetermined_imaginary_parts_are_trimmed_from_the_back() {
        // z + conj(z) = 10 and z - conj(z) = 4i both split into real and imag
        // equations: 4 equations over {z_r, z_i}. The excess is trimmed by
        // dropping unmatched "(imag)" equations from the back (Java
        // trimExcessEquations); "(real)" equations are never trimmed.
        let equations = parse_equations("z + conj(z) = 10\nz - conj(z) = 4i");
        let expanded = expand_complex(equations, true).unwrap();
        assert_eq!(expanded.len(), 3);
        let texts: Vec<&str> = expanded.iter().map(|e| e.source_text.as_str()).collect();
        assert!(texts.contains(&"z + conj(z) = 10 (real)"));
        assert!(texts.contains(&"z - conj(z) = 4i (real)"));
        // Exactly one imag equation survived the trim.
        assert_eq!(
            texts.iter().filter(|t| t.ends_with("(imag)")).count(),
            1,
            "one (imag) equation should remain: {texts:?}"
        );
    }

    #[test]
    fn comparisons_are_real_valued() {
        let e = Expr::Compare {
            op: crate::ast::CmpOp::Lt,
            left: Box::new(Expr::var("z")),
            right: Box::new(Expr::num(2.0)),
        };
        assert_eq!(
            real_part(&e).unwrap(),
            Expr::Compare {
                op: crate::ast::CmpOp::Lt,
                left: Box::new(Expr::var("z_r")),
                right: Box::new(Expr::num(2.0)),
            }
        );
        assert_eq!(imag_part(&e).unwrap(), Expr::num(0.0));
    }

    #[test]
    fn array_access_suffixes_the_name_but_not_the_indices() {
        let e = Expr::ArrayAccess {
            name: "a".into(),
            indices: vec![Expr::var("n")],
        };
        assert_eq!(
            real_part(&e).unwrap(),
            Expr::ArrayAccess {
                name: "a_r".into(),
                indices: vec![Expr::var("n")],
            }
        );
        assert_eq!(
            imag_part(&e).unwrap(),
            Expr::ArrayAccess {
                name: "a_i".into(),
                indices: vec![Expr::var("n")],
            }
        );
    }
}
