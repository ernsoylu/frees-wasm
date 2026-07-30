//! Symbolic partial differentiation of [`Expr`] trees.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/ast/Differentiator.java`
//! (536 LOC), rule for rule. The Java class is a static recursive walker with
//! algebraic simplification applied *during construction* (`0 + x → x`,
//! `1 * x → x`, `x^0 → 1`, constant folding of `+ - * /` over two numeric
//! literals — but deliberately **no** folding of `^`). This port mirrors both
//! the rule table and the construction order of every derivative expression so
//! the output tree shapes match the oracle's.
//!
//! Where the Java code returns `null` — property calls (`prop$…`), user
//! procedures (`proc$…`), eigen synthetics (`eigen$…`), `integral`,
//! multi-argument intrinsics like `atan2`/`mod`/`min`/`max`, comparisons,
//! array machinery, or any function name it does not know — this port returns
//! [`None`]. Notably the Java table only knows the `arcsin`/`arccos`/`arctan`
//! spellings (not `asin`/`acos`/`atan`, which the evaluator also accepts):
//! that asymmetry is preserved because it is the oracle's behavior.

use crate::ast::{BinOp, Expr};

/// Java `Differentiator.GAMMA`.
const GAMMA: &str = "gamma";
/// Java `Differentiator.DIGAMMA`.
const DIGAMMA: &str = "digamma";

/// The partial derivative `∂expr/∂var` as a new expression tree, or `None`
/// where the expression is not symbolically differentiable (an intrinsic the
/// table does not cover, a table lookup, a property call). `None` is the
/// signal for the caller to fall back to a finite-difference entry — the Java
/// `NewtonSolver.computeJacobian` contract.
///
/// `var` is the lowercase canonical variable name (lowercased here again
/// defensively, exactly as `Differentiator.differentiate` does).
pub fn differentiate(expr: &Expr, var: &str) -> Option<Expr> {
    let var = var.to_ascii_lowercase();
    diff(expr, &var)
}

// ── core recursive differentiator ───────────────────────────────────────────

fn diff(e: &Expr, var: &str) -> Option<Expr> {
    match e {
        // The unit annotation and imaginary flag do not survive: the Java arm
        // returns a bare `num(0)` for every literal.
        Expr::Num { .. } => Some(num(0.0)),
        Expr::Var(name) => Some(num(if name == var { 1.0 } else { 0.0 })),
        Expr::Neg(operand) => diff(operand, var).map(simplify_neg),
        Expr::BinOp { op, left, right } => diff_bin_op(*op, left, right, var),
        Expr::Call { function, args } => diff_call(function, args, var),
        // Non-differentiable constructs.
        Expr::Str(_)
        | Expr::ArrayAccess { .. }
        | Expr::Range { .. }
        | Expr::ArrayLiteral(_)
        | Expr::Compare { .. }
        | Expr::Logical { .. }
        | Expr::Not(_) => None,
    }
}

// ── binary operators ────────────────────────────────────────────────────────

fn diff_bin_op(op: BinOp, f: &Expr, g: &Expr, var: &str) -> Option<Expr> {
    match op {
        BinOp::Add => {
            let df = diff(f, var)?;
            let dg = diff(g, var)?;
            Some(simplify_add(df, dg))
        }
        BinOp::Sub => {
            let df = diff(f, var)?;
            let dg = diff(g, var)?;
            Some(simplify_sub(df, dg))
        }
        BinOp::Mul => {
            // Product rule: f'g + fg'
            let df = diff(f, var)?;
            let dg = diff(g, var)?;
            Some(simplify_add(
                simplify_mul(df, g.clone()),
                simplify_mul(f.clone(), dg),
            ))
        }
        BinOp::Div => {
            // Quotient rule: (f'g − fg') / g²
            let df = diff(f, var)?;
            let dg = diff(g, var)?;
            let numerator = simplify_sub(simplify_mul(df, g.clone()), simplify_mul(f.clone(), dg));
            let denominator = simplify_mul(g.clone(), g.clone());
            Some(simplify_div(numerator, denominator))
        }
        BinOp::Pow => diff_power(f, g, var),
        // The Java switch has no case for `\` or the element-wise operators;
        // they fall through to `default -> null`.
        BinOp::LeftDiv | BinOp::ElemMul | BinOp::ElemDiv | BinOp::ElemLeftDiv | BinOp::ElemPow => {
            None
        }
    }
}

/// Power rule.
///
/// * Constant exponent: `d/dx f^n = n * f^(n−1) * f'`
/// * General case: `d/dx f^g = f^g * (g' ln f + g f'/f)`
fn diff_power(f: &Expr, g: &Expr, var: &str) -> Option<Expr> {
    let df = diff(f, var)?;
    let dg = diff(g, var)?;

    if is_constant(g) {
        // n * f^(n-1) * f'
        let n_minus_one = simplify_sub(g.clone(), num(1.0));
        return Some(simplify_mul(
            simplify_mul(g.clone(), simplify_pow(f.clone(), n_minus_one)),
            df,
        ));
    }
    // General: f^g * (g' * ln(f) + g * f'/f)
    let ln_f = call1("ln", f.clone());
    let term1 = simplify_mul(dg, ln_f);
    let term2 = simplify_mul(g.clone(), simplify_div(df, f.clone()));
    Some(simplify_mul(
        simplify_pow(f.clone(), g.clone()),
        simplify_add(term1, term2),
    ))
}

// ── function calls (chain rule) ─────────────────────────────────────────────

fn diff_call(function: &str, args: &[Expr], var: &str) -> Option<Expr> {
    // Property calls, user-defined functions, special synthetic calls.
    if function.starts_with("prop$")
        || function.starts_with("proc$")
        || function.starts_with("eigen$")
    {
        return None;
    }

    match function {
        // ── trig ────────────────────────────────────────────────────
        "sin" => chain_rule(args, var, |f| call1("cos", f)),
        "cos" => chain_rule(args, var, |f| simplify_neg(call1("sin", f))),
        "tan" => chain_rule(args, var, |f| {
            simplify_div(
                num(1.0),
                simplify_mul(call1("cos", f.clone()), call1("cos", f)),
            )
        }),

        // ── inverse trig ────────────────────────────────────────────
        "arcsin" => chain_rule(args, var, |f| {
            simplify_div(
                num(1.0),
                call1("sqrt", simplify_sub(num(1.0), simplify_mul(f.clone(), f))),
            )
        }),
        "arccos" => chain_rule(args, var, |f| {
            simplify_neg(simplify_div(
                num(1.0),
                call1("sqrt", simplify_sub(num(1.0), simplify_mul(f.clone(), f))),
            ))
        }),
        "arctan" => chain_rule(args, var, |f| {
            simplify_div(num(1.0), simplify_add(num(1.0), simplify_mul(f.clone(), f)))
        }),

        // ── exp / log ───────────────────────────────────────────────
        "exp" => chain_rule(args, var, |f| call1("exp", f)),
        "ln" => chain_rule(args, var, |f| simplify_div(num(1.0), f)),
        "log10" => chain_rule(args, var, |f| {
            simplify_div(num(1.0), simplify_mul(f, call1("ln", num(10.0))))
        }),
        "log2" => chain_rule(args, var, |f| {
            simplify_div(num(1.0), simplify_mul(f, call1("ln", num(2.0))))
        }),
        "sqrt" => chain_rule(args, var, |f| {
            simplify_div(num(1.0), simplify_mul(num(2.0), call1("sqrt", f)))
        }),
        // d/dx cbrt(f) = f' / (3 · cbrt(f)²)
        "cbrt" => chain_rule(args, var, |f| {
            simplify_div(
                num(1.0),
                simplify_mul(
                    num(3.0),
                    simplify_mul(call1("cbrt", f.clone()), call1("cbrt", f)),
                ),
            )
        }),

        // ── abs ─────────────────────────────────────────────────────
        // d/dx |f| = f / |f| * f'  (= sign(f) * f')
        "abs" => chain_rule(args, var, |f| simplify_div(f.clone(), call1("abs", f))),

        // ── error functions ─────────────────────────────────────────
        // d/dx erf(f) = (2/√π) exp(−f²) f'
        "erf" => chain_rule(args, var, |f| {
            simplify_mul(
                simplify_div(num(2.0), call1("sqrt", num(std::f64::consts::PI))),
                call1("exp", simplify_neg(simplify_mul(f.clone(), f))),
            )
        }),
        // d/dx erfc(f) = −(2/√π) exp(−f²) f'
        "erfc" => chain_rule(args, var, |f| {
            simplify_neg(simplify_mul(
                simplify_div(num(2.0), call1("sqrt", num(std::f64::consts::PI))),
                call1("exp", simplify_neg(simplify_mul(f.clone(), f))),
            ))
        }),

        // ── gamma ───────────────────────────────────────────────────
        // d/dx Γ(f) = Γ(f) * ψ(f) * f'   (ψ = digamma)
        GAMMA => chain_rule(args, var, |f| {
            simplify_mul(call1(GAMMA, f.clone()), call1(DIGAMMA, f))
        }),
        // d/dx lnΓ(f) = ψ(f) * f'
        "loggamma" => chain_rule(args, var, |f| call1(DIGAMMA, f)),
        // digamma itself is evaluated numerically at runtime; it only
        // appears as an intermediate inside a Jacobian expression.

        // d/dx erfinv(f) = (√π/2) exp(erfinv(f)²) f'
        "erfinv" => chain_rule(args, var, |f| {
            let inv = call1("erfinv", f);
            simplify_mul(
                simplify_div(call1("sqrt", num(std::f64::consts::PI)), num(2.0)),
                call1("exp", simplify_mul(inv.clone(), inv)),
            )
        }),

        // ∂/∂x B(a,b) = B(a,b) [(ψ(a) − ψ(a+b)) a' + (ψ(b) − ψ(a+b)) b']
        "beta" => diff_beta(args, var),

        // d/dx J_n(x) = (J_{n−1}(x) − J_{n+1}(x)) / 2 · x'  (constant n)
        "besselj" | "bessel_j" => diff_bessel(args, var, function, true, false),
        // d/dx I_n(x) = (I_{n−1}(x) + I_{n+1}(x)) / 2 · x'  (constant n)
        "besseli" | "bessel_i" => diff_bessel(args, var, function, false, false),
        // d/dx Y_n(x) = (Y_{n−1}(x) − Y_{n+1}(x)) / 2 · x'  (constant n)
        "bessely" | "bessel_y" => diff_bessel(args, var, function, true, false),
        // d/dx K_n(x) = -(K_{n−1}(x) + K_{n+1}(x)) / 2 · x'  (constant n)
        "besselk" | "bessel_k" => diff_bessel(args, var, function, false, true),

        // Shortcut Bessel functions: J₀′ = −J₁, Y₀′ = −Y₁, K₀′ = −K₁ (the Java
        // cases are separate but textually identical), and I₀′ = +I₁.
        "besselj0" | "bessel_j0" | "bessely0" | "bessel_y0" | "besselk0" | "bessel_k0" => {
            chain_rule(args, var, |f| {
                simplify_neg(call1(sibling(function, "1"), f))
            })
        }
        "besseli0" | "bessel_i0" => chain_rule(args, var, |f| call1(sibling(function, "1"), f)),

        // J₁′ = J₀ − J₁/x, I₁′ = I₀ − I₁/x, Y₁′ = Y₀ − Y₁/x (identical Java
        // bodies), and K₁′ = −K₀ − K₁/x.
        "besselj1" | "bessel_j1" | "besseli1" | "bessel_i1" | "bessely1" | "bessel_y1" => {
            chain_rule(args, var, |f| {
                simplify_sub(
                    call1(sibling(function, "0"), f.clone()),
                    simplify_div(call1(function, f.clone()), f),
                )
            })
        }
        "besselk1" | "bessel_k1" => chain_rule(args, var, |f| {
            simplify_sub(
                simplify_neg(call1(sibling(function, "0"), f.clone())),
                simplify_div(call1(function, f.clone()), f),
            )
        }),

        // Chi-Square distribution derivative: the PDF
        "chi_square" => diff_chi_square(args, var),
        "random" | "randg" | "uncertaintyof" => Some(num(0.0)),
        "probability" => diff_probability(args, var),

        // ── hyperbolic functions ────────────────────────────────────
        "sinh" => chain_rule(args, var, |f| call1("cosh", f)),
        "cosh" => chain_rule(args, var, |f| call1("sinh", f)),
        "tanh" => chain_rule(args, var, |f| {
            simplify_sub(
                num(1.0),
                simplify_mul(call1("tanh", f.clone()), call1("tanh", f)),
            )
        }),
        "arcsinh" => chain_rule(args, var, |f| {
            simplify_div(
                num(1.0),
                call1("sqrt", simplify_add(simplify_mul(f.clone(), f), num(1.0))),
            )
        }),
        "arccosh" => chain_rule(args, var, |f| {
            simplify_div(
                num(1.0),
                call1("sqrt", simplify_sub(simplify_mul(f.clone(), f), num(1.0))),
            )
        }),
        "arctanh" => chain_rule(args, var, |f| {
            simplify_div(num(1.0), simplify_sub(num(1.0), simplify_mul(f.clone(), f)))
        }),

        // ── rounding and piecewise ─────────────────────────────────
        // Piecewise-constant: derivative is 0 wherever it is defined. (The
        // Java arm does not inspect the argument at all.)
        "floor" | "ceil" | "trunc" | "sign" | "step" => Some(num(0.0)),
        "round" => diff_round(args, var),
        "factorial" => diff_factorial(args, var),

        // ── conditionals & series ───────────────────────────────────
        "if" => diff_if_call(args, var),
        "sum" => diff_sum(args, var),
        "product" => diff_product(args, var),

        // ── complex helpers in real mode ────────────────────────────
        "conj" => {
            if args.len() != 1 {
                return None;
            }
            diff(&args[0], var)
        }
        "magnitude" => chain_rule(args, var, |f| simplify_div(f.clone(), call1("abs", f))),
        "angle" | "anglerad" | "angledeg" => {
            if args.len() != 1 {
                return None;
            }
            diff(&args[0], var)?;
            Some(num(0.0))
        }
        "cis" => chain_rule(args, var, |f| simplify_neg(call1("sin", f))),

        // ── unsupported multi-arg or procedural functions ────────────
        "integral" | "min" | "max" | "average" | "avg" | "atan2" | "mod" | "gcd" | "lcm"
        | "bitand" | "bitor" | "bitxor" | "bitnot" | "bitshiftl" | "bitshiftr" | "baseconvert"
        | DIGAMMA | "real" | "imag" => None,

        // Unknown function → cannot differentiate
        _ => None,
    }
}

/// `function` with its trailing `0`/`1` order digit replaced — the Java
/// `function.substring(0, function.length() - 1) + digit` idiom used by the
/// shortcut Bessel rules (`besselj0 → besselj1`, `bessel_k1 → bessel_k0`).
fn sibling(function: &str, digit: &str) -> String {
    format!("{}{digit}", &function[..function.len() - 1])
}

fn diff_chi_square(args: &[Expr], var: &str) -> Option<Expr> {
    if args.len() != 2 {
        return None;
    }
    let x = &args[0];
    let df = &args[1];
    if !is_constant(df) {
        return None;
    }
    let dx = diff(x, var)?;
    let half_df = simplify_div(df.clone(), num(2.0));
    let numerator = simplify_mul(
        call1("exp", simplify_neg(simplify_div(x.clone(), num(2.0)))),
        simplify_pow(x.clone(), simplify_sub(half_df.clone(), num(1.0))),
    );
    let denominator = simplify_mul(
        simplify_pow(num(2.0), half_df.clone()),
        call1(GAMMA, half_df),
    );
    Some(simplify_mul(simplify_div(numerator, denominator), dx))
}

fn diff_probability(args: &[Expr], var: &str) -> Option<Expr> {
    if args.len() != 4 {
        return None;
    }
    let x1 = &args[0];
    let x2 = &args[1];
    let mean = &args[2];
    let std_dev = &args[3];
    if !is_constant(mean) || !is_constant(std_dev) {
        return None;
    }
    let dx1 = diff(x1, var)?;
    let dx2 = diff(x2, var)?;
    let factor = simplify_mul(std_dev.clone(), num(libm::sqrt(2.0 * std::f64::consts::PI)));
    // exp(−(x − mean)² / (2 σ²)) / (σ √(2π)), built exactly like the two
    // longhand Java expressions.
    let normal_pdf = |x: &Expr| {
        simplify_div(
            call1(
                "exp",
                simplify_neg(simplify_div(
                    simplify_pow(simplify_sub(x.clone(), mean.clone()), num(2.0)),
                    simplify_mul(num(2.0), simplify_pow(std_dev.clone(), num(2.0))),
                )),
            ),
            factor.clone(),
        )
    };
    let pdf1 = normal_pdf(x1);
    let pdf2 = normal_pdf(x2);
    Some(simplify_sub(
        simplify_mul(pdf2, dx2),
        simplify_mul(pdf1, dx1),
    ))
}

fn diff_round(args: &[Expr], var: &str) -> Option<Expr> {
    if args.is_empty() || args.len() > 2 {
        return None;
    }
    diff(&args[0], var)?;
    Some(num(0.0))
}

fn diff_factorial(args: &[Expr], var: &str) -> Option<Expr> {
    if args.len() != 1 {
        return None;
    }
    let f = &args[0];
    let df = diff(f, var)?;
    let fact = call1("factorial", f.clone());
    let digamma = call1(DIGAMMA, simplify_add(f.clone(), num(1.0)));
    Some(simplify_mul(simplify_mul(fact, digamma), df))
}

fn diff_if_call(args: &[Expr], var: &str) -> Option<Expr> {
    if args.len() != 5 {
        return None;
    }
    let dx = diff(&args[2], var)?;
    let dy = diff(&args[3], var)?;
    let dz = diff(&args[4], var)?;
    Some(Expr::call(
        "if",
        vec![args[0].clone(), args[1].clone(), dx, dy, dz],
    ))
}

fn diff_sum(args: &[Expr], var: &str) -> Option<Expr> {
    if args.len() == 4 && matches!(args[0], Expr::Var(_)) {
        let d_term = diff(&args[3], var)?;
        return Some(Expr::call(
            "sum",
            vec![args[0].clone(), args[1].clone(), args[2].clone(), d_term],
        ));
    }
    let mut d_args = Vec::with_capacity(args.len());
    for arg in args {
        d_args.push(diff(arg, var)?);
    }
    Some(Expr::call("sum", d_args))
}

fn diff_product(args: &[Expr], var: &str) -> Option<Expr> {
    if !(args.len() == 4 && matches!(args[0], Expr::Var(_))) {
        return None;
    }
    let lower = &args[1];
    let upper = &args[2];
    let term = &args[3];
    let d_term = diff(term, var)?;
    let prod = Expr::call("product", args.to_vec());
    let sum_term = simplify_div(d_term, term.clone());
    let sum = Expr::call(
        "sum",
        vec![args[0].clone(), lower.clone(), upper.clone(), sum_term],
    );
    Some(simplify_mul(prod, sum))
}

/// ∂/∂x B(a,b) = B(a,b) [(ψ(a) − ψ(a+b)) a' + (ψ(b) − ψ(a+b)) b'].
fn diff_beta(args: &[Expr], var: &str) -> Option<Expr> {
    if args.len() != 2 {
        return None;
    }
    let a = &args[0];
    let b = &args[1];
    let da = diff(a, var)?;
    let db = diff(b, var)?;
    let psi_sum = call1(DIGAMMA, simplify_add(a.clone(), b.clone()));
    let term_a = simplify_mul(simplify_sub(call1(DIGAMMA, a.clone()), psi_sum.clone()), da);
    let term_b = simplify_mul(simplify_sub(call1(DIGAMMA, b.clone()), psi_sum), db);
    Some(simplify_mul(
        Expr::call("beta", vec![a.clone(), b.clone()]),
        simplify_add(term_a, term_b),
    ))
}

/// Bessel recurrence derivative for `besselj(x, n)` / `besseli(x, n)` with a
/// constant order n:  J′_n = (J_{n−1} − J_{n+1})/2,  I′_n = (I_{n−1} + I_{n+1})/2.
fn diff_bessel(
    args: &[Expr],
    var: &str,
    function: &str,
    subtract: bool,
    negate: bool,
) -> Option<Expr> {
    if args.len() != 2 {
        return None;
    }
    let x = &args[0];
    let order = &args[1];
    if !is_constant(order) {
        return None;
    }
    let dx = diff(x, var)?;
    let lower = Expr::call(
        function,
        vec![x.clone(), simplify_sub(order.clone(), num(1.0))],
    );
    let upper = Expr::call(
        function,
        vec![x.clone(), simplify_add(order.clone(), num(1.0))],
    );
    let combined = if subtract {
        simplify_sub(lower, upper)
    } else {
        simplify_add(lower, upper)
    };
    let result = simplify_mul(simplify_div(combined, num(2.0)), dx);
    Some(if negate { simplify_neg(result) } else { result })
}

/// Applies the chain rule for a single-argument function:
/// `d/dx h(f(x)) = h'(f) * f'(x)`. `outer_derivative` maps the inner
/// expression `f` to `h'(f)` (the Java `OuterDerivative` functional interface).
fn chain_rule(
    args: &[Expr],
    var: &str,
    outer_derivative: impl FnOnce(Expr) -> Expr,
) -> Option<Expr> {
    if args.len() != 1 {
        return None;
    }
    let f = &args[0];
    let df = diff(f, var)?;
    let outer = outer_derivative(f.clone());
    Some(simplify_mul(outer, df))
}

// ── simplification ──────────────────────────────────────────────────────────

/// Returns true if the expression does not depend on any variable.
fn is_constant(e: &Expr) -> bool {
    e.variables().is_empty()
}

/// The Java pattern `Expr.Num(double v, String u, boolean i) && v == 0.0`
/// matches regardless of unit or imaginary flag; so does this one.
fn is_zero(e: &Expr) -> bool {
    matches!(e, Expr::Num { value, .. } if *value == 0.0)
}

fn is_one(e: &Expr) -> bool {
    matches!(e, Expr::Num { value, .. } if *value == 1.0)
}

fn simplify_add(a: Expr, b: Expr) -> Expr {
    if is_zero(&a) {
        return b;
    }
    if is_zero(&b) {
        return a;
    }
    // Fold two numeric constants.
    if let (Expr::Num { value: va, .. }, Expr::Num { value: vb, .. }) = (&a, &b) {
        return num(va + vb);
    }
    Expr::bin(BinOp::Add, a, b)
}

fn simplify_sub(a: Expr, b: Expr) -> Expr {
    if is_zero(&b) {
        return a;
    }
    if is_zero(&a) {
        return simplify_neg(b);
    }
    if let (Expr::Num { value: va, .. }, Expr::Num { value: vb, .. }) = (&a, &b) {
        return num(va - vb);
    }
    Expr::bin(BinOp::Sub, a, b)
}

fn simplify_mul(a: Expr, b: Expr) -> Expr {
    if is_zero(&a) || is_zero(&b) {
        return num(0.0);
    }
    if is_one(&a) {
        return b;
    }
    if is_one(&b) {
        return a;
    }
    if let (Expr::Num { value: va, .. }, Expr::Num { value: vb, .. }) = (&a, &b) {
        return num(va * vb);
    }
    Expr::bin(BinOp::Mul, a, b)
}

fn simplify_div(a: Expr, b: Expr) -> Expr {
    if is_zero(&a) {
        return num(0.0);
    }
    if is_one(&b) {
        return a;
    }
    if let (Expr::Num { value: va, .. }, Expr::Num { value: vb, .. }) = (&a, &b) {
        if *vb != 0.0 {
            return num(va / vb);
        }
    }
    Expr::bin(BinOp::Div, a, b)
}

fn simplify_pow(base: Expr, exp: Expr) -> Expr {
    if is_zero(&exp) {
        return num(1.0);
    }
    if is_one(&exp) {
        return base;
    }
    Expr::bin(BinOp::Pow, base, exp)
}

fn simplify_neg(a: Expr) -> Expr {
    if is_zero(&a) {
        return num(0.0);
    }
    if let Expr::Neg(inner) = a {
        return *inner;
    }
    if let Expr::Num { value, .. } = a {
        return num(-value);
    }
    Expr::Neg(Box::new(a))
}

// ── helpers ─────────────────────────────────────────────────────────────────

/// Java `num(double)`: a unitless, non-imaginary literal.
fn num(value: f64) -> Expr {
    Expr::num(value)
}

/// Java `call(String, Expr)`: a one-argument call node.
fn call1(function: impl AsRef<str>, arg: Expr) -> Expr {
    Expr::call(function, vec![arg])
}

// ── tests (simplification helpers are private; the full DifferentiatorTest
//    port lives in tests/differentiator.rs) ─────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn var(n: &str) -> Expr {
        Expr::var(n)
    }
    fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
        Expr::bin(op, a, b)
    }

    // Java: oneMulXSimplifiesToX
    #[test]
    fn one_mul_x_simplifies_to_x() {
        assert_eq!(simplify_mul(num(1.0), var("x")), var("x"));
    }

    // Java: zeroMulXSimplifiesToZero
    #[test]
    fn zero_mul_x_simplifies_to_zero() {
        assert_eq!(simplify_mul(num(0.0), var("x")), num(0.0));
    }

    // Java: xPowZeroSimplifiesToOne
    #[test]
    fn x_pow_zero_simplifies_to_one() {
        assert_eq!(simplify_pow(var("x"), num(0.0)), num(1.0));
    }

    // Java: xPowOneSimplifiesToX
    #[test]
    fn x_pow_one_simplifies_to_x() {
        assert_eq!(simplify_pow(var("x"), num(1.0)), var("x"));
    }

    // Java: zeroDivXSimplifiesToZero
    #[test]
    fn zero_div_x_simplifies_to_zero() {
        assert_eq!(simplify_div(num(0.0), var("x")), num(0.0));
    }

    // Java: negOfZeroSimplifiesToZero
    #[test]
    fn neg_of_zero_simplifies_to_zero() {
        assert_eq!(simplify_neg(num(0.0)), num(0.0));
    }

    // Java: doubleNegSimplifies
    #[test]
    fn double_neg_simplifies() {
        assert_eq!(simplify_neg(Expr::Neg(Box::new(var("x")))), var("x"));
    }

    #[test]
    fn numeric_literals_fold_for_add_sub_mul_div_but_not_pow() {
        assert_eq!(simplify_add(num(2.0), num(3.0)), num(5.0));
        assert_eq!(simplify_sub(num(2.0), num(3.0)), num(-1.0));
        assert_eq!(simplify_mul(num(2.0), num(3.0)), num(6.0));
        assert_eq!(simplify_div(num(6.0), num(3.0)), num(2.0));
        // Java's simplifyPow has no constant-folding branch.
        assert_eq!(
            simplify_pow(num(2.0), num(3.0)),
            bin(BinOp::Pow, num(2.0), num(3.0))
        );
    }

    #[test]
    fn division_by_literal_zero_is_not_folded() {
        // Java guards `vb != 0.0` and falls through to a BinOp node.
        assert_eq!(
            simplify_div(num(1.0), num(0.0)),
            bin(BinOp::Div, num(1.0), num(0.0))
        );
    }

    #[test]
    fn neg_folds_a_numeric_literal() {
        assert_eq!(simplify_neg(num(-5.0)), num(5.0));
        assert_eq!(simplify_neg(var("x")), Expr::Neg(Box::new(var("x"))));
    }

    #[test]
    fn zero_with_a_unit_still_counts_as_zero() {
        // The Java record pattern ignores the unit and imaginary components.
        let zero_metres = Expr::Num {
            value: 0.0,
            unit: Some("m".into()),
            is_imaginary: false,
        };
        assert_eq!(simplify_add(zero_metres, var("x")), var("x"));
    }

    #[test]
    fn sub_of_zero_left_negates() {
        assert_eq!(
            simplify_sub(num(0.0), var("x")),
            Expr::Neg(Box::new(var("x")))
        );
    }

    // ── output-shape locks (construction order matches the Java tree) ──

    #[test]
    fn quadratic_derivative_has_java_shape() {
        // d/dx x^2 → simplifyMul(simplifyMul(2, x^1→x), 1) → 2 * x
        let d = differentiate(&bin(BinOp::Pow, var("x"), num(2.0)), "x").unwrap();
        assert_eq!(d, bin(BinOp::Mul, num(2.0), var("x")));
    }

    #[test]
    fn cubic_derivative_has_java_shape() {
        // d/dx x^3 → 3 * x^2 (the trailing ·1 is simplified away)
        let d = differentiate(&bin(BinOp::Pow, var("x"), num(3.0)), "x").unwrap();
        assert_eq!(
            d,
            bin(BinOp::Mul, num(3.0), bin(BinOp::Pow, var("x"), num(2.0)))
        );
    }

    #[test]
    fn sin_of_x_squared_has_java_shape() {
        // d/dx sin(x²) = cos(x²) * (2 * x)
        let x_sq = bin(BinOp::Pow, var("x"), num(2.0));
        let d = differentiate(&Expr::call("sin", vec![x_sq.clone()]), "x").unwrap();
        assert_eq!(
            d,
            bin(
                BinOp::Mul,
                Expr::call("cos", vec![x_sq]),
                bin(BinOp::Mul, num(2.0), var("x"))
            )
        );
    }

    #[test]
    fn besselk_order_one_has_java_shape() {
        // d/dx K₁(x) = −((K₀(x) + K₂(x)) / 2)
        let expr = Expr::call("besselk", vec![var("x"), num(1.0)]);
        let d = differentiate(&expr, "x").unwrap();
        let k = |n: f64| Expr::call("besselk", vec![var("x"), num(n)]);
        assert_eq!(
            d,
            Expr::Neg(Box::new(bin(
                BinOp::Div,
                bin(BinOp::Add, k(0.0), k(2.0)),
                num(2.0)
            )))
        );
    }

    #[test]
    fn left_division_and_element_wise_operators_are_not_differentiable() {
        // Java's switch on the op char has no case for these → default null.
        for op in [
            BinOp::LeftDiv,
            BinOp::ElemMul,
            BinOp::ElemDiv,
            BinOp::ElemLeftDiv,
            BinOp::ElemPow,
        ] {
            assert_eq!(differentiate(&bin(op, var("x"), var("x")), "x"), None);
        }
    }

    #[test]
    fn variable_argument_is_lowercased_like_java() {
        // Java: `variable.toLowerCase()` at the entry point.
        assert_eq!(differentiate(&var("x"), "X"), Some(num(1.0)));
    }
}
