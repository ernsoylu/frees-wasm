//! Port of `../frEES/backend/core/src/test/java/com/frees/backend/ast/DifferentiatorTest.java`
//! (660 LOC) — every case, plus extra Java-parity checks on the rule table's
//! edges (arity guards, the deliberate `null` arms, the shortcut-Bessel and
//! chi-square conventions).
//!
//! # Two evaluators
//!
//! The Java test verifies derivatives numerically through the engine's own
//! `Evaluator`. Here:
//!
//! * exact-value assertions go through the real [`frees_core::eval::eval`]
//!   (the same evaluator the symbolic-Jacobian path will use), and
//! * the central-difference helper uses a **test-local evaluator** below,
//!   because several derivative trees contain kernels the engine evaluator
//!   does not provide yet (`digamma`, `erfinv`, `besseli`/`besselk`,
//!   `chi_square`). The local kernels follow the same definitions as the Java
//!   oracle's (Apache Commons): `chi_square(x, df)` is the regularized lower
//!   incomplete gamma `P(df/2, x/2)` (a CDF — its derivative is the PDF),
//!   `besselj(x, n)` is `J_n(x)`, `probability` is the erf difference.

use frees_core::ast::{BinOp, Expr};
use frees_core::differentiator::differentiate;

// ── AST builder helpers (mirroring the Java test's) ─────────────────────────

fn num(v: f64) -> Expr {
    Expr::num(v)
}
fn var(n: &str) -> Expr {
    Expr::var(n)
}
fn neg(e: Expr) -> Expr {
    Expr::Neg(Box::new(e))
}
fn add(a: Expr, b: Expr) -> Expr {
    Expr::bin(BinOp::Add, a, b)
}
fn sub(a: Expr, b: Expr) -> Expr {
    Expr::bin(BinOp::Sub, a, b)
}
fn mul(a: Expr, b: Expr) -> Expr {
    Expr::bin(BinOp::Mul, a, b)
}
fn div(a: Expr, b: Expr) -> Expr {
    Expr::bin(BinOp::Div, a, b)
}
fn pow(a: Expr, b: Expr) -> Expr {
    Expr::bin(BinOp::Pow, a, b)
}
fn call(f: &str, args: Vec<Expr>) -> Expr {
    Expr::call(f, args)
}

// The engine's own alias — it carries a non-default hasher (eval.rs), so a
// local `HashMap<String, f64>` no longer satisfies `eval`.
use frees_core::eval::Scope;

fn scope(pairs: &[(&str, f64)]) -> Scope {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

/// Evaluate through the real engine evaluator (exact-value assertions).
///
/// Safety note: despite the name (kept to mirror the Java test helper), this
/// is `frees_core::eval::eval` — a pure numeric interpreter over the typed
/// `Expr` AST. It executes no code and touches no host environment.
fn eval(e: &Expr, sc: &Scope) -> f64 {
    frees_core::eval::eval(e, sc).expect("expression should evaluate")
}

fn eval_at(e: &Expr, name: &str, value: f64) -> f64 {
    eval(e, &scope(&[(name, value)]))
}

// ── the numerical-verification helper (Java assertDerivativeNumerically) ───

/// Verifies that the analytical derivative matches a central-difference
/// approximation at the given point (h = 1e-7, like the Java helper).
fn assert_derivative_numerically(expr: &Expr, var_name: &str, point: &[(&str, f64)], tol: f64) {
    let deriv = differentiate(expr, var_name)
        .unwrap_or_else(|| panic!("expected differentiable expression: {expr:?}"));
    let sc = scope(point);
    let analytical = test_eval(&deriv, &sc);

    let h = 1e-7;
    let x0 = *sc
        .get(var_name)
        .unwrap_or_else(|| panic!("point does not bind {var_name}"));
    let mut plus = sc.clone();
    let mut minus = sc.clone();
    plus.insert(var_name.to_string(), x0 + h);
    minus.insert(var_name.to_string(), x0 - h);
    let numerical = (test_eval(expr, &plus) - test_eval(expr, &minus)) / (2.0 * h);

    assert!(
        (numerical - analytical).abs() <= tol,
        "analytical derivative {analytical} vs numerical {numerical} \
         for d/d{var_name} of {expr:?} at {point:?} (deriv tree: {deriv:?})"
    );
}

// ── test-local evaluator ────────────────────────────────────────────────────

fn test_eval(e: &Expr, sc: &Scope) -> f64 {
    match e {
        Expr::Num { value, .. } => *value,
        Expr::Var(name) => *sc
            .get(name)
            .unwrap_or_else(|| panic!("test scope has no value for {name}")),
        Expr::Neg(inner) => -test_eval(inner, sc),
        Expr::BinOp { op, left, right } => {
            let l = test_eval(left, sc);
            let r = test_eval(right, sc);
            match op {
                BinOp::Add => l + r,
                BinOp::Sub => l - r,
                BinOp::Mul => l * r,
                BinOp::Div => l / r,
                BinOp::Pow => libm::pow(l, r),
                other => panic!("operator {other:?} not needed by these tests"),
            }
        }
        Expr::Call { function, args } => test_eval_call(function, args, sc),
        other => panic!("node {other:?} not needed by these tests"),
    }
}

fn test_eval_call(function: &str, args: &[Expr], sc: &Scope) -> f64 {
    let a = |i: usize| test_eval(&args[i], sc);
    match function {
        "sin" => libm::sin(a(0)),
        "cos" => libm::cos(a(0)),
        "tan" => libm::tan(a(0)),
        "arcsin" => libm::asin(a(0)),
        "arccos" => libm::acos(a(0)),
        "arctan" => libm::atan(a(0)),
        "exp" => libm::exp(a(0)),
        "ln" => libm::log(a(0)),
        "log10" => libm::log10(a(0)),
        "log2" => libm::log(a(0)) / libm::log(2.0),
        "sqrt" => libm::sqrt(a(0)),
        "cbrt" => libm::cbrt(a(0)),
        "abs" | "magnitude" => libm::fabs(a(0)),
        "erf" => libm::erf(a(0)),
        "erfc" => libm::erfc(a(0)),
        "erfinv" => erfinv(a(0)),
        "gamma" => libm::tgamma(a(0)),
        "loggamma" => libm::lgamma(a(0)),
        "factorial" => libm::tgamma(a(0) + 1.0),
        "digamma" => digamma(a(0)),
        "beta" => {
            let (x, y) = (a(0), a(1));
            libm::exp(libm::lgamma(x) + libm::lgamma(y) - libm::lgamma(x + y))
        }
        "sinh" => libm::sinh(a(0)),
        "cosh" => libm::cosh(a(0)),
        "tanh" => libm::tanh(a(0)),
        "arcsinh" => libm::asinh(a(0)),
        "arccosh" => libm::acosh(a(0)),
        "arctanh" => libm::atanh(a(0)),
        // Argument convention matches the Java Evaluator: besselj(x, n) = J_n(x).
        "besselj" => libm::jn(a(1) as i32, a(0)),
        "bessely" => libm::yn(a(1) as i32, a(0)),
        "besseli" => bessel_i(a(1) as i32, a(0)),
        "besselk" => bessel_k(a(1) as i32, a(0)),
        "besselj0" => libm::j0(a(0)),
        "besselj1" => libm::j1(a(0)),
        "bessely0" => libm::y0(a(0)),
        "bessely1" => libm::y1(a(0)),
        "besseli0" => bessel_i(0, a(0)),
        "besseli1" => bessel_i(1, a(0)),
        "besselk0" => bessel_k(0, a(0)),
        "besselk1" => bessel_k(1, a(0)),
        // Java: Gamma.regularizedGammaP(df/2, x/2), 0 for x <= 0 — the CDF.
        "chi_square" => {
            let (x, df) = (a(0), a(1));
            if x <= 0.0 {
                0.0
            } else {
                regularized_gamma_p(df / 2.0, x / 2.0)
            }
        }
        // Java: 0.5 (erf((x2−μ)/(σ√2)) − erf((x1−μ)/(σ√2))).
        "probability" => {
            let (x1, x2, m, s) = (a(0), a(1), a(2), a(3));
            let d = s * std::f64::consts::SQRT_2;
            0.5 * (libm::erf((x2 - m) / d) - libm::erf((x1 - m) / d))
        }
        "conj" => a(0),
        "angle" | "anglerad" => libm::atan2(0.0, a(0)),
        "cis" => libm::cos(a(0)),
        other => panic!("function `{other}` not needed by these tests"),
    }
}

// ── local special-function kernels (double precision at the test points) ───

/// ψ(x) for x > 0: recurrence up to x ≥ 10, then the asymptotic expansion.
fn digamma(mut x: f64) -> f64 {
    assert!(x > 0.0, "test digamma only handles x > 0, got {x}");
    let mut result = 0.0;
    while x < 10.0 {
        result -= 1.0 / x;
        x += 1.0;
    }
    let inv = 1.0 / x;
    let inv2 = inv * inv;
    // ψ(x) ≈ ln x − 1/(2x) − 1/(12x²) + 1/(120x⁴) − 1/(252x⁶) + 1/(240x⁸) − 1/(132x¹⁰)
    result += libm::log(x)
        - 0.5 * inv
        - inv2
            * (1.0 / 12.0
                - inv2
                    * (1.0 / 120.0
                        - inv2 * (1.0 / 252.0 - inv2 * (1.0 / 240.0 - inv2 * (1.0 / 132.0)))));
    result
}

/// erf⁻¹(y) by Newton iteration on erf — machine precision for |y| < 1.
fn erfinv(y: f64) -> f64 {
    assert!(y.abs() < 1.0, "test erfinv needs |y| < 1, got {y}");
    let two_over_sqrt_pi = 2.0 / libm::sqrt(std::f64::consts::PI);
    let mut t = 0.0_f64;
    for _ in 0..100 {
        let step = (libm::erf(t) - y) / (two_over_sqrt_pi * libm::exp(-t * t));
        t -= step;
        if libm::fabs(step) <= 1e-15 * (1.0 + libm::fabs(t)) {
            break;
        }
    }
    t
}

/// I_n(x) for integer n ≥ 0 by the ascending series — machine precision for
/// the small arguments these tests use.
fn bessel_i(n: i32, x: f64) -> f64 {
    assert!(n >= 0, "test bessel_i only handles n >= 0, got {n}");
    let nf = f64::from(n);
    let half = x / 2.0;
    let mut term = libm::pow(half, nf) / libm::tgamma(nf + 1.0);
    let mut sum = term;
    let q = half * half;
    for k in 1..200 {
        let kf = f64::from(k);
        term *= q / (kf * (kf + nf));
        sum += term;
        if term <= libm::fabs(sum) * 1e-17 {
            break;
        }
    }
    sum
}

/// K_n(x) for integer n ≥ 0: K₀ by the ascending series (A&S 9.6.13), K₁ from
/// the Wronskian I₀K₁ + I₁K₀ = 1/x, then upward recurrence
/// K_{m+1} = K_{m−1} + (2m/x)·K_m.
fn bessel_k(n: i32, x: f64) -> f64 {
    assert!(n >= 0, "test bessel_k only handles n >= 0, got {n}");
    assert!(x > 0.0, "test bessel_k needs x > 0, got {x}");
    const EULER_GAMMA: f64 = 0.577_215_664_901_532_9;
    let half = x / 2.0;
    let q = half * half;
    let i0 = bessel_i(0, x);
    // Σ_{k≥1} q^k/(k!)² · H_k
    let mut sum = 0.0;
    let mut term = 1.0;
    let mut harmonic = 0.0;
    for k in 1..200 {
        let kf = f64::from(k);
        term *= q / (kf * kf);
        harmonic += 1.0 / kf;
        let contribution = term * harmonic;
        sum += contribution;
        if contribution <= libm::fabs(sum) * 1e-17 {
            break;
        }
    }
    let k0 = -(libm::log(half) + EULER_GAMMA) * i0 + sum;
    if n == 0 {
        return k0;
    }
    let k1 = (1.0 / x - bessel_i(1, x) * k0) / i0;
    let mut km1 = k0;
    let mut km = k1;
    for m in 1..n {
        let next = km1 + 2.0 * f64::from(m) / x * km;
        km1 = km;
        km = next;
    }
    km
}

/// Regularized lower incomplete gamma P(a, x) — series for x < a+1, modified
/// Lentz continued fraction otherwise (the classic `gammp` construction, the
/// same function Apache's `Gamma.regularizedGammaP` computes).
fn regularized_gamma_p(a: f64, x: f64) -> f64 {
    assert!(a > 0.0 && x >= 0.0, "test gammp needs a > 0, x >= 0");
    if x == 0.0 {
        return 0.0;
    }
    let log_prefactor = -x + a * libm::log(x) - libm::lgamma(a);
    if x < a + 1.0 {
        let mut ap = a;
        let mut sum = 1.0 / a;
        let mut del = sum;
        for _ in 0..500 {
            ap += 1.0;
            del *= x / ap;
            sum += del;
            if libm::fabs(del) < libm::fabs(sum) * 1e-17 {
                break;
            }
        }
        sum * libm::exp(log_prefactor)
    } else {
        let tiny = 1e-300;
        let mut b = x + 1.0 - a;
        let mut c = 1.0 / tiny;
        let mut d = 1.0 / b;
        let mut h = d;
        for i in 1..500 {
            let an = -f64::from(i) * (f64::from(i) - a);
            b += 2.0;
            d = an * d + b;
            if libm::fabs(d) < tiny {
                d = tiny;
            }
            c = b + an / c;
            if libm::fabs(c) < tiny {
                c = tiny;
            }
            d = 1.0 / d;
            let delta = d * c;
            h *= delta;
            if libm::fabs(delta - 1.0) < 1e-17 {
                break;
            }
        }
        1.0 - libm::exp(log_prefactor) * h
    }
}

#[test]
fn local_kernels_match_reference_values() {
    // Spot-checks against published values so the FD harness itself is honest.
    assert!((digamma(1.0) + 0.577_215_664_901_532_9).abs() < 1e-12); // ψ(1) = −γ
    assert!((digamma(2.0) - (1.0 - 0.577_215_664_901_532_9)).abs() < 1e-12);
    assert!((erfinv(0.5) - 0.476_936_276_204_469_9).abs() < 1e-12);
    assert!((bessel_i(0, 2.0) - 2.279_585_302_336_067).abs() < 1e-12); // I₀(2)
    assert!((bessel_i(1, 2.0) - 1.590_636_854_637_329).abs() < 1e-12); // I₁(2)
    assert!((bessel_k(0, 1.5) - 0.213_805_562_647_526_8).abs() < 1e-12); // K₀(1.5)
    assert!((bessel_k(1, 1.5) - 0.277_387_800_456_782_1).abs() < 1e-12); // K₁(1.5)
    assert!((regularized_gamma_p(1.0, 2.0) - (1.0 - libm::exp(-2.0))).abs() < 1e-14);
    assert!((regularized_gamma_p(2.5, 1.0) - 0.150_854_963_915_390_3).abs() < 1e-12);
}

// ── basic derivatives ───────────────────────────────────────────────────────

#[test]
fn constant_derivative_is_zero() {
    let d = differentiate(&num(42.0), "x").expect("differentiable");
    assert_eq!(eval_at(&d, "x", 5.0), 0.0);
}

#[test]
fn variable_derivative_is_one_for_match_and_zero_for_other() {
    let dx = differentiate(&var("x"), "x").expect("differentiable");
    let dy = differentiate(&var("x"), "y").expect("differentiable");
    assert_eq!(eval_at(&dx, "x", 3.0), 1.0);
    assert_eq!(eval(&dy, &scope(&[("x", 3.0), ("y", 1.0)])), 0.0);
}

#[test]
fn case_insensitive_variable_names() {
    // Variable names are case-insensitive (lowercased by the Var constructor).
    let d = differentiate(&var("X"), "x").expect("differentiable");
    assert_eq!(eval_at(&d, "x", 7.0), 1.0);
}

#[test]
fn linear_expression() {
    // d/dx (3x + 5) = 3
    let expr = add(mul(num(3.0), var("x")), num(5.0));
    let d = differentiate(&expr, "x").expect("differentiable");
    assert!((eval_at(&d, "x", 99.0) - 3.0).abs() <= 1e-12);
}

#[test]
fn negation_derivative() {
    // d/dx (-x) = -1
    let d = differentiate(&neg(var("x")), "x").expect("differentiable");
    assert_eq!(eval_at(&d, "x", 5.0), -1.0);
}

#[test]
fn subtraction_derivative() {
    // d/dx (x - x^2) = 1 - 2x
    let expr = sub(var("x"), pow(var("x"), num(2.0)));
    assert_derivative_numerically(&expr, "x", &[("x", 3.0)], 1e-6);
}

// ── polynomial derivatives ──────────────────────────────────────────────────

#[test]
fn quadratic_polynomial() {
    // d/dx (x^2) = 2x
    let expr = pow(var("x"), num(2.0));
    let d = differentiate(&expr, "x").expect("differentiable");
    assert!((eval_at(&d, "x", 3.0) - 6.0).abs() <= 1e-12);
    assert!((eval_at(&d, "x", 5.0) - 10.0).abs() <= 1e-12);
}

#[test]
fn cubic_polynomial() {
    // d/dx (x^3) = 3x^2
    let expr = pow(var("x"), num(3.0));
    assert_derivative_numerically(&expr, "x", &[("x", 2.0)], 1e-6);
}

#[test]
fn polynomial_multiple_terms() {
    // d/dx (2x^3 - 5x^2 + 3x - 7) = 6x^2 - 10x + 3
    let expr = sub(
        add(
            sub(
                mul(num(2.0), pow(var("x"), num(3.0))),
                mul(num(5.0), pow(var("x"), num(2.0))),
            ),
            mul(num(3.0), var("x")),
        ),
        num(7.0),
    );
    let x = 4.0;
    let expected = 6.0 * x * x - 10.0 * x + 3.0; // 96 - 40 + 3 = 59
    let d = differentiate(&expr, "x").expect("differentiable");
    assert!((eval_at(&d, "x", x) - expected).abs() <= 1e-10);
}

// ── product rule ────────────────────────────────────────────────────────────

#[test]
fn product_rule() {
    // d/dx (x * x^2) = d/dx(x^3) = 3x^2
    let expr = mul(var("x"), pow(var("x"), num(2.0)));
    assert_derivative_numerically(&expr, "x", &[("x", 2.5)], 1e-6);
}

#[test]
fn product_rule_two_variables() {
    // d/dx (x * y) = y
    let expr = mul(var("x"), var("y"));
    let dx = differentiate(&expr, "x").expect("differentiable");
    assert!((eval(&dx, &scope(&[("x", 3.0), ("y", 7.0)])) - 7.0).abs() <= 1e-12);
}

// ── quotient rule ───────────────────────────────────────────────────────────

#[test]
fn quotient_rule() {
    // d/dx (x / (x+1)) = 1/(x+1)^2
    let expr = div(var("x"), add(var("x"), num(1.0)));
    assert_derivative_numerically(&expr, "x", &[("x", 3.0)], 1e-6);
}

// ── power rule ──────────────────────────────────────────────────────────────

#[test]
fn general_power_rule() {
    // d/dx (x^x) at x=2:  x^x (ln x + 1)
    let expr = pow(var("x"), var("x"));
    assert_derivative_numerically(&expr, "x", &[("x", 2.0)], 1e-5);
}

#[test]
fn constant_base_power_rule() {
    // d/dx (2^x) = 2^x * ln(2)
    let expr = pow(num(2.0), var("x"));
    assert_derivative_numerically(&expr, "x", &[("x", 3.0)], 1e-5);
}

// ── chain rule with built-in functions ──────────────────────────────────────

#[test]
fn sin_of_x_squared() {
    // d/dx sin(x^2) = cos(x^2) * 2x
    let expr = call("sin", vec![pow(var("x"), num(2.0))]);
    assert_derivative_numerically(&expr, "x", &[("x", 1.5)], 1e-6);
}

#[test]
fn cos_derivative() {
    // d/dx cos(x) = -sin(x)
    let expr = call("cos", vec![var("x")]);
    let d = differentiate(&expr, "x").expect("differentiable");
    let x = 1.0;
    assert!((eval_at(&d, "x", x) - (-libm::sin(x))).abs() <= 1e-12);
}

#[test]
fn tan_derivative() {
    // d/dx tan(x) = 1/cos²(x)
    let expr = call("tan", vec![var("x")]);
    assert_derivative_numerically(&expr, "x", &[("x", 0.5)], 1e-6);
}

#[test]
fn exp_of_2x() {
    // d/dx exp(2x) = 2 * exp(2x)
    let expr = call("exp", vec![mul(num(2.0), var("x"))]);
    assert_derivative_numerically(&expr, "x", &[("x", 1.0)], 1e-6);
}

#[test]
fn ln_of_x_plus_1() {
    // d/dx ln(x+1) = 1/(x+1)
    let expr = call("ln", vec![add(var("x"), num(1.0))]);
    let d = differentiate(&expr, "x").expect("differentiable");
    let x = 3.0;
    assert!((eval_at(&d, "x", x) - 1.0 / (x + 1.0)).abs() <= 1e-12);
}

#[test]
fn log10_derivative() {
    // d/dx log10(x) = 1/(x ln10)
    let expr = call("log10", vec![var("x")]);
    assert_derivative_numerically(&expr, "x", &[("x", 5.0)], 1e-6);
}

#[test]
fn sqrt_derivative() {
    // d/dx sqrt(x) = 1/(2 sqrt(x))
    let expr = call("sqrt", vec![var("x")]);
    let d = differentiate(&expr, "x").expect("differentiable");
    let x = 4.0;
    assert!((eval_at(&d, "x", x) - 1.0 / (2.0 * libm::sqrt(x))).abs() <= 1e-12);
}

#[test]
fn sqrt_chain_rule() {
    // d/dx sqrt(x^2 + 1) = x / sqrt(x^2 + 1)
    let expr = call("sqrt", vec![add(pow(var("x"), num(2.0)), num(1.0))]);
    assert_derivative_numerically(&expr, "x", &[("x", 3.0)], 1e-6);
}

// ── inverse trig ────────────────────────────────────────────────────────────

#[test]
fn arcsin_derivative() {
    let expr = call("arcsin", vec![var("x")]);
    assert_derivative_numerically(&expr, "x", &[("x", 0.5)], 1e-6);
}

#[test]
fn arccos_derivative() {
    let expr = call("arccos", vec![var("x")]);
    assert_derivative_numerically(&expr, "x", &[("x", 0.5)], 1e-6);
}

#[test]
fn arctan_derivative() {
    let expr = call("arctan", vec![var("x")]);
    assert_derivative_numerically(&expr, "x", &[("x", 2.0)], 1e-6);
}

// ── abs ─────────────────────────────────────────────────────────────────────

#[test]
fn abs_derivative_positive() {
    // d/dx |x| at x=3 → sign(3) = 1
    let expr = call("abs", vec![var("x")]);
    assert_derivative_numerically(&expr, "x", &[("x", 3.0)], 1e-6);
}

#[test]
fn abs_derivative_negative() {
    // d/dx |x| at x=-3 → sign(-3) = -1
    let expr = call("abs", vec![var("x")]);
    assert_derivative_numerically(&expr, "x", &[("x", -3.0)], 1e-6);
}

// ── special functions ───────────────────────────────────────────────────────

#[test]
fn erf_derivative() {
    // d/dx erf(x) = (2/√π) exp(-x²)
    let expr = call("erf", vec![var("x")]);
    assert_derivative_numerically(&expr, "x", &[("x", 1.0)], 1e-6);
}

#[test]
fn erfc_derivative() {
    // d/dx erfc(x) = -(2/√π) exp(-x²)
    let expr = call("erfc", vec![var("x")]);
    assert_derivative_numerically(&expr, "x", &[("x", 0.5)], 1e-6);
}

#[test]
fn erf_chain_rule() {
    // d/dx erf(2x) = (2/√π) exp(-4x²) * 2
    let expr = call("erf", vec![mul(num(2.0), var("x"))]);
    assert_derivative_numerically(&expr, "x", &[("x", 0.5)], 1e-5);
}

#[test]
fn gamma_derivative() {
    // d/dx Γ(x) = Γ(x) * ψ(x)  -- verified numerically
    let expr = call("gamma", vec![var("x")]);
    assert_derivative_numerically(&expr, "x", &[("x", 3.0)], 1e-4);
}

#[test]
fn log_gamma_derivative() {
    // d/dx lnΓ(x) = ψ(x)
    let expr = call("loggamma", vec![var("x")]);
    assert_derivative_numerically(&expr, "x", &[("x", 3.0)], 1e-5);
}

#[test]
fn erf_inv_derivative() {
    // d/dx erfinv(x) = (√π/2) exp(erfinv(x)²)
    let expr = call("erfinv", vec![var("x")]);
    assert_derivative_numerically(&expr, "x", &[("x", 0.5)], 1e-5);
}

#[test]
fn beta_derivative() {
    // ∂/∂a B(a,b) = B(a,b)(ψ(a) − ψ(a+b)), and the same through both args
    let expr = call("beta", vec![var("a"), var("b")]);
    assert_derivative_numerically(&expr, "a", &[("a", 2.0), ("b", 3.0)], 1e-6);
    assert_derivative_numerically(&expr, "b", &[("a", 2.0), ("b", 3.0)], 1e-6);
    // Chain rule through a composite first argument
    let composite = call("beta", vec![mul(var("x"), num(2.0)), num(3.0)]);
    assert_derivative_numerically(&composite, "x", &[("x", 1.5)], 1e-6);
}

#[test]
fn bessel_j_derivative() {
    // d/dx J_n(x) = (J_{n−1}(x) − J_{n+1}(x)) / 2, constant order
    let expr = call("besselj", vec![var("x"), num(1.0)]);
    assert_derivative_numerically(&expr, "x", &[("x", 2.5)], 1e-6);
}

#[test]
fn bessel_i_derivative() {
    // d/dx I_n(x) = (I_{n−1}(x) + I_{n+1}(x)) / 2, constant order
    let expr = call("besseli", vec![var("x"), num(1.0)]);
    assert_derivative_numerically(&expr, "x", &[("x", 2.0)], 1e-6);
}

#[test]
fn bessel_with_variable_order_is_not_differentiable() {
    // The recurrence derivative only holds for a constant order.
    let expr = call("besselj", vec![var("x"), var("n")]);
    assert_eq!(differentiate(&expr, "x"), None);
}

#[test]
fn new_bessel_and_chi_square_derivatives() {
    let k = call("besselk", vec![var("x"), num(1.0)]);
    assert_derivative_numerically(&k, "x", &[("x", 1.5)], 1e-6);

    let y = call("bessely", vec![var("x"), num(1.0)]);
    assert_derivative_numerically(&y, "x", &[("x", 2.0)], 1e-6);

    // Shortcut functions
    let j0 = call("besselj0", vec![var("x")]);
    assert_derivative_numerically(&j0, "x", &[("x", 2.0)], 1e-6);

    let j1 = call("besselj1", vec![var("x")]);
    assert_derivative_numerically(&j1, "x", &[("x", 2.0)], 1e-6);

    let i0 = call("besseli0", vec![var("x")]);
    assert_derivative_numerically(&i0, "x", &[("x", 2.0)], 1e-6);

    let i1 = call("besseli1", vec![var("x")]);
    assert_derivative_numerically(&i1, "x", &[("x", 2.0)], 1e-6);

    let k0 = call("besselk0", vec![var("x")]);
    assert_derivative_numerically(&k0, "x", &[("x", 1.5)], 1e-6);

    let k1 = call("besselk1", vec![var("x")]);
    assert_derivative_numerically(&k1, "x", &[("x", 1.5)], 1e-6);

    let y0 = call("bessely0", vec![var("x")]);
    assert_derivative_numerically(&y0, "x", &[("x", 2.0)], 1e-6);

    let y1 = call("bessely1", vec![var("x")]);
    assert_derivative_numerically(&y1, "x", &[("x", 2.0)], 1e-6);

    // Chi-Square
    let chi2 = call("chi_square", vec![var("x"), num(2.0)]);
    assert_derivative_numerically(&chi2, "x", &[("x", 4.0)], 1e-6);
}

// ── unsupported expressions return None ─────────────────────────────────────

#[test]
fn property_call_returns_none() {
    let expr = call("prop$enthalpy$r134a$t$x", vec![num(300.0), num(1.0)]);
    assert_eq!(differentiate(&expr, "x"), None);
}

#[test]
fn procedure_call_returns_none() {
    let expr = call("proc$myfunc$0", vec![var("x")]);
    assert_eq!(differentiate(&expr, "x"), None);
}

#[test]
fn eigen_call_returns_none() {
    let expr = call(
        "eigen$val$0$2",
        vec![num(1.0), num(0.0), num(0.0), num(1.0)],
    );
    assert_eq!(differentiate(&expr, "x"), None);
}

#[test]
fn integral_returns_none() {
    let expr = call("integral", vec![var("x"), var("t"), num(0.0), num(1.0)]);
    assert_eq!(differentiate(&expr, "x"), None);
}

#[test]
fn compare_expr_returns_none() {
    let expr = Expr::Compare {
        op: frees_core::ast::CmpOp::Lt,
        left: Box::new(var("x")),
        right: Box::new(num(5.0)),
    };
    assert_eq!(differentiate(&expr, "x"), None);
}

#[test]
fn array_access_returns_none() {
    let expr = Expr::ArrayAccess {
        name: "a".into(),
        indices: vec![var("x")],
    };
    assert_eq!(differentiate(&expr, "x"), None);
}

// ── simplification quality ──────────────────────────────────────────────────

#[test]
fn derivative_of_constant_simplifies_to_zero_literal() {
    let d = differentiate(&num(7.0), "x").expect("differentiable");
    assert_eq!(d, num(0.0));
}

#[test]
fn derivative_of_variable_simplifies_to_one_literal() {
    let d = differentiate(&var("x"), "x").expect("differentiable");
    assert_eq!(d, num(1.0));
}

#[test]
fn zero_plus_x_simplifies_to_x() {
    // d/dx (5 + x) should not contain a "0 + 1" sub-tree: it is just 1.
    let d = differentiate(&add(num(5.0), var("x")), "x").expect("differentiable");
    assert_eq!(d, num(1.0));
}

#[test]
fn x_plus_zero_simplifies_to_x() {
    // d/dx (x + 5) = 1 + 0 → should simplify to 1.
    let d = differentiate(&add(var("x"), num(5.0)), "x").expect("differentiable");
    assert_eq!(d, num(1.0));
}

// ── partial derivatives (multivariable) ─────────────────────────────────────

#[test]
fn partial_derivative_xy() {
    // f(x,y) = x^2 * y + 3*y^2;  ∂f/∂x = 2x*y,  ∂f/∂y = x^2 + 6y
    let expr = add(
        mul(pow(var("x"), num(2.0)), var("y")),
        mul(num(3.0), pow(var("y"), num(2.0))),
    );
    let pt = [("x", 2.0), ("y", 3.0)];
    assert_derivative_numerically(&expr, "x", &pt, 1e-6);
    assert_derivative_numerically(&expr, "y", &pt, 1e-6);
}

// ── composition of multiple functions ───────────────────────────────────────

#[test]
fn composition_exp_sin() {
    // d/dx exp(sin(x)) = exp(sin(x)) * cos(x)
    let expr = call("exp", vec![call("sin", vec![var("x")])]);
    assert_derivative_numerically(&expr, "x", &[("x", 1.0)], 1e-6);
}

#[test]
fn composition_ln_sqrt() {
    // d/dx ln(sqrt(x)) = 1/(2x)
    let expr = call("ln", vec![call("sqrt", vec![var("x")])]);
    let d = differentiate(&expr, "x").expect("differentiable");
    let x = 4.0;
    assert!((eval_at(&d, "x", x) - 1.0 / (2.0 * x)).abs() <= 1e-12);
}

// ── derivative of residual (lhs - rhs) as used by the Newton solver ────────

#[test]
fn residual_derivative() {
    // Equation: x^2 + y = 5  →  residual = x^2 + y - 5
    // d(residual)/dx = 2x,  d(residual)/dy = 1
    let lhs = add(pow(var("x"), num(2.0)), var("y"));
    let rhs = num(5.0);
    let residual = sub(lhs, rhs);

    let dx = differentiate(&residual, "x").expect("differentiable");
    let dy = differentiate(&residual, "y").expect("differentiable");

    let pt = scope(&[("x", 3.0), ("y", 1.0)]);
    assert!((eval(&dx, &pt) - 6.0).abs() <= 1e-12);
    assert!((eval(&dy, &pt) - 1.0).abs() <= 1e-12);
}

// ── null propagation through binary operators ───────────────────────────────

#[test]
fn none_propagates_through_add() {
    // If a sub-expression can't be differentiated, the whole thing is None.
    let expr = add(
        var("x"),
        call("prop$h$water$t$p", vec![var("x"), num(100.0)]),
    );
    assert_eq!(differentiate(&expr, "x"), None);
}

#[test]
fn none_propagates_through_mul() {
    let expr = mul(var("x"), call("proc$myfunc$0", vec![var("x")]));
    assert_eq!(differentiate(&expr, "x"), None);
}

#[test]
fn differentiates_hyperbolic_functions() {
    assert_derivative_numerically(&call("sinh", vec![var("x")]), "x", &[("x", 1.0)], 1e-6);
    assert_derivative_numerically(&call("cosh", vec![var("x")]), "x", &[("x", 1.0)], 1e-6);
    assert_derivative_numerically(&call("tanh", vec![var("x")]), "x", &[("x", 1.0)], 1e-6);
    assert_derivative_numerically(&call("arcsinh", vec![var("x")]), "x", &[("x", 1.0)], 1e-6);
    assert_derivative_numerically(&call("arccosh", vec![var("x")]), "x", &[("x", 2.0)], 1e-6);
    assert_derivative_numerically(&call("arctanh", vec![var("x")]), "x", &[("x", 0.5)], 1e-6);
}

#[test]
fn differentiates_piecewise_and_rounding() {
    for name in ["floor", "ceil", "trunc", "sign", "step", "round"] {
        let d = differentiate(&call(name, vec![var("x")]), "x")
            .unwrap_or_else(|| panic!("{name} should differentiate to zero"));
        assert_eq!(eval_at(&d, "x", 1.5), 0.0, "{name}");
    }
    // Two-argument round is also piecewise-constant.
    let rd2 = call("round", vec![var("x"), num(2.0)]);
    let d = differentiate(&rd2, "x").expect("differentiable");
    assert_eq!(eval_at(&d, "x", 1.5), 0.0);

    // factorial (Gamma-based): Factorial(x) = Gamma(x+1)
    assert_derivative_numerically(&call("factorial", vec![var("x")]), "x", &[("x", 2.0)], 1e-4);
}

#[test]
fn differentiates_conditionals_and_series() {
    // If(a, b, x^2, y, z) → w.r.t x → If(a, b, 2x, 0, 0)
    let if_expr = call(
        "if",
        vec![
            num(1.0),
            num(2.0),
            pow(var("x"), num(2.0)),
            var("y"),
            var("z"),
        ],
    );
    let d_if = differentiate(&if_expr, "x").expect("differentiable");
    assert_eq!(
        eval(&d_if, &scope(&[("x", 2.0), ("y", 10.0), ("z", 20.0)])),
        4.0
    );

    // Sum(i, 1, 3, i*x^2) → w.r.t x → Sum(i, 1, 3, 2*i*x) = 2x + 4x + 6x = 12x
    let sum_expr = call(
        "sum",
        vec![
            var("i"),
            num(1.0),
            num(3.0),
            mul(var("i"), pow(var("x"), num(2.0))),
        ],
    );
    let d_sum = differentiate(&sum_expr, "x").expect("differentiable");
    assert!((eval(&d_sum, &scope(&[("x", 2.0)])) - 24.0).abs() <= 1e-9);

    // Product(i, 1, 3, x) → product of 3 x's = x^3 → derivative 3x^2
    let prod_expr = call("product", vec![var("i"), num(1.0), num(3.0), var("x")]);
    let d_prod = differentiate(&prod_expr, "x").expect("differentiable");
    assert!((eval(&d_prod, &scope(&[("x", 2.0)])) - 12.0).abs() <= 1e-9);
}

#[test]
fn differentiates_complex_helpers_in_real_mode() {
    assert_derivative_numerically(&call("conj", vec![var("x")]), "x", &[("x", 2.0)], 1e-6);
    assert_derivative_numerically(&call("magnitude", vec![var("x")]), "x", &[("x", 2.0)], 1e-6);
    let d_angle = differentiate(&call("angle", vec![var("x")]), "x").expect("differentiable");
    assert_eq!(eval_at(&d_angle, "x", 2.0), 0.0);
    assert_derivative_numerically(&call("cis", vec![var("x")]), "x", &[("x", 1.0)], 1e-6);
}

// ── extra Java-parity checks beyond the oracle test file ────────────────────

#[test]
fn asin_acos_atan_spellings_are_not_in_the_java_table() {
    // The Java switch only knows arcsin/arccos/arctan; the evaluator's
    // asin/acos/atan aliases fall to `default -> null`. Preserved verbatim.
    for name in ["asin", "acos", "atan"] {
        assert_eq!(
            differentiate(&call(name, vec![var("x")]), "x"),
            None,
            "{name}"
        );
    }
}

#[test]
fn explicitly_unsupported_functions_return_none() {
    let two = |name: &str| call(name, vec![var("x"), num(2.0)]);
    for expr in [
        two("atan2"),
        two("mod"),
        two("gcd"),
        two("lcm"),
        call("min", vec![var("x"), num(1.0)]),
        call("max", vec![var("x"), num(1.0)]),
        call("average", vec![var("x"), num(1.0)]),
        call("avg", vec![var("x"), num(1.0)]),
        call("digamma", vec![var("x")]),
        call("real", vec![var("x")]),
        call("imag", vec![var("x")]),
        call("baseconvert", vec![var("x"), num(16.0), num(10.0)]),
        call("bitand", vec![var("x"), num(3.0)]),
        call("some_unknown_function", vec![var("x")]),
    ] {
        assert_eq!(differentiate(&expr, "x"), None, "{expr:?}");
    }
}

#[test]
fn random_family_differentiates_to_zero_without_inspecting_args() {
    // Java: `case "random", "randg", "uncertaintyof" -> num(0.0);`
    for name in ["random", "randg", "uncertaintyof"] {
        let expr = call(name, vec![var("x")]);
        assert_eq!(differentiate(&expr, "x"), Some(num(0.0)), "{name}");
    }
}

#[test]
fn piecewise_constant_rules_ignore_undifferentiable_arguments() {
    // The Java arm for floor/ceil/trunc/sign/step returns num(0) without
    // differentiating the argument — even a property call inside is fine.
    let expr = call("floor", vec![call("prop$h$water$t$p", vec![var("x")])]);
    assert_eq!(differentiate(&expr, "x"), Some(num(0.0)));
    // `round`, by contrast, does differentiate its argument first.
    let round_of_prop = call("round", vec![call("prop$h$water$t$p", vec![var("x")])]);
    assert_eq!(differentiate(&round_of_prop, "x"), None);
}

#[test]
fn arity_guards_return_none() {
    // round with 0 or 3 args
    assert_eq!(differentiate(&call("round", vec![]), "x"), None);
    assert_eq!(
        differentiate(&call("round", vec![var("x"), num(1.0), num(2.0)]), "x"),
        None
    );
    // if: the Java rule requires exactly the five-argument form
    assert_eq!(
        differentiate(&call("if", vec![var("x"), num(1.0), num(2.0)]), "x"),
        None
    );
    // conj/angle with the wrong arity
    assert_eq!(
        differentiate(&call("conj", vec![var("x"), var("y")]), "x"),
        None
    );
    assert_eq!(
        differentiate(&call("angle", vec![var("x"), var("y")]), "x"),
        None
    );
    // sin with two args is rejected by the chain-rule helper
    assert_eq!(
        differentiate(&call("sin", vec![var("x"), var("y")]), "x"),
        None
    );
    // beta/bessel/chi_square with the wrong arity
    assert_eq!(differentiate(&call("beta", vec![var("x")]), "x"), None);
    assert_eq!(differentiate(&call("besselj", vec![var("x")]), "x"), None);
    assert_eq!(
        differentiate(&call("chi_square", vec![var("x")]), "x"),
        None
    );
}

#[test]
fn chi_square_with_variable_df_is_not_differentiable() {
    let expr = call("chi_square", vec![var("x"), var("k")]);
    assert_eq!(differentiate(&expr, "x"), None);
}

#[test]
fn probability_derivative_matches_finite_differences() {
    // probability(x, 3, 1, 2) w.r.t. x — the lower bound moves.
    let expr = call("probability", vec![var("x"), num(3.0), num(1.0), num(2.0)]);
    assert_derivative_numerically(&expr, "x", &[("x", 0.5)], 1e-6);
    // And through the upper bound.
    let upper = call("probability", vec![num(0.0), var("x"), num(1.0), num(2.0)]);
    assert_derivative_numerically(&upper, "x", &[("x", 2.5)], 1e-6);
}

#[test]
fn probability_with_nonconstant_parameters_is_not_differentiable() {
    let expr = call("probability", vec![num(0.0), num(3.0), var("m"), num(2.0)]);
    assert_eq!(differentiate(&expr, "x"), None);
    let expr2 = call("probability", vec![num(0.0), num(3.0), num(1.0), var("s")]);
    assert_eq!(differentiate(&expr2, "x"), None);
}

#[test]
fn log2_and_cbrt_derivatives() {
    assert_derivative_numerically(&call("log2", vec![var("x")]), "x", &[("x", 5.0)], 1e-6);
    assert_derivative_numerically(&call("cbrt", vec![var("x")]), "x", &[("x", 2.0)], 1e-6);
    // and through a composite argument
    let composite = call("cbrt", vec![add(pow(var("x"), num(2.0)), num(1.0))]);
    assert_derivative_numerically(&composite, "x", &[("x", 1.5)], 1e-6);
}

#[test]
fn sum_without_binding_form_differentiates_each_argument() {
    // sum(x, x^2) has no (var, lo, hi, body) shape → every arg differentiates:
    // d/dx = 1 + 2x = 7 at x = 3. Evaluated through the real engine evaluator.
    let expr = call("sum", vec![var("x"), pow(var("x"), num(2.0))]);
    let d = differentiate(&expr, "x").expect("differentiable");
    assert!((eval_at(&d, "x", 3.0) - 7.0).abs() <= 1e-12);
}

#[test]
fn sum_binding_form_keeps_bounds_underived() {
    // d/dx sum(i, x, 3, i) → the bounds are carried over verbatim, only the
    // body is differentiated (to 0 here). Exact Java tree shape.
    let expr = call("sum", vec![var("i"), var("x"), num(3.0), var("i")]);
    let d = differentiate(&expr, "x").expect("differentiable");
    assert_eq!(d, call("sum", vec![var("i"), var("x"), num(3.0), num(0.0)]));
}

#[test]
fn product_without_binding_form_is_not_differentiable() {
    let expr = call("product", vec![var("x"), var("y")]);
    assert_eq!(differentiate(&expr, "x"), None);
}

#[test]
fn unit_annotated_constant_exponent_still_folds() {
    // A literal exponent carrying a unit is still `isConstant`; n−1 folds to a
    // plain number exactly as in Java (units do not survive folding).
    let annotated_two = Expr::Num {
        value: 2.0,
        unit: Some("m".into()),
        is_imaginary: false,
    };
    let d = differentiate(&pow(var("x"), annotated_two.clone()), "x").expect("differentiable");
    assert_eq!(d, mul(annotated_two, var("x")));
    assert!((eval_at(&d, "x", 3.0) - 6.0).abs() <= 1e-12);
}

#[test]
fn string_and_boolean_nodes_are_not_differentiable() {
    assert_eq!(differentiate(&Expr::Str("hello".into()), "x"), None);
    let logical = Expr::Logical {
        op: frees_core::ast::LogicOp::And,
        left: Box::new(var("x")),
        right: Box::new(num(1.0)),
    };
    assert_eq!(differentiate(&logical, "x"), None);
    assert_eq!(differentiate(&Expr::Not(Box::new(var("x"))), "x"), None);
    let range = Expr::Range {
        start: Box::new(num(1.0)),
        end: Box::new(var("x")),
    };
    assert_eq!(differentiate(&range, "x"), None);
    assert_eq!(
        differentiate(&Expr::ArrayLiteral(vec![var("x")]), "x"),
        None
    );
}

#[test]
fn underscore_bessel_spellings_differentiate_too() {
    // The Java table lists bessel_j / bessel_j0 / … alongside the compact
    // spellings; the sibling-name rewriting must preserve the underscore form.
    let expr = call("bessel_j0", vec![var("x")]);
    let d = differentiate(&expr, "x").expect("differentiable");
    assert_eq!(d, neg(call("bessel_j1", vec![var("x")])));

    let expr = call("bessel_k1", vec![var("x")]);
    let d = differentiate(&expr, "x").expect("differentiable");
    assert_eq!(
        d,
        sub(
            neg(call("bessel_k0", vec![var("x")])),
            div(call("bessel_k1", vec![var("x")]), var("x"))
        )
    );

    let expr = call("bessel_i", vec![var("x"), num(1.0)]);
    assert_derivative_numerically_with_aliases(&expr, "x", &[("x", 2.0)], 1e-6);
}

/// Like [`assert_derivative_numerically`] but resolving the `bessel_*`
/// underscore aliases the local evaluator maps to the same kernels.
fn assert_derivative_numerically_with_aliases(
    expr: &Expr,
    var_name: &str,
    point: &[(&str, f64)],
    tol: f64,
) {
    fn strip(e: &Expr) -> Expr {
        match e {
            Expr::Call { function, args } => {
                let name = function.replace("bessel_", "bessel");
                Expr::call(name, args.iter().map(strip).collect())
            }
            Expr::BinOp { op, left, right } => Expr::bin(*op, strip(left), strip(right)),
            Expr::Neg(inner) => Expr::Neg(Box::new(strip(inner))),
            other => other.clone(),
        }
    }
    assert_derivative_numerically(&strip(expr), var_name, point, tol);
}
