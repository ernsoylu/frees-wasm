//! Expression evaluation.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/ast/Evaluator.java`
//! (2,053 LOC, 226 dispatch arms).
//!
//! # What this pass covers
//!
//! Every [`Expr`] variant, every [`BinOp`] / [`CmpOp`] / [`LogicOp`], the full
//! `ConstantsRegistry` table, and the *scalar elementary* half of the Java
//! dispatch: arithmetic, rounding, trig/hyperbolic (and their inverses), logs,
//! the special functions `libm` provides, bit/number theory, descriptive
//! statistics, the orthogonal-polynomial recurrences and the two binding forms
//! `sum` / `product`.
//!
//! Everything else in the Java `evalCall` chain — fluid properties (`prop$…`),
//! user `FUNCTION`/`PROCEDURE` dispatch (`proc$…`), matrix/eigen/FFT synthetics,
//! control systems, TABLE/parametric/ODE accessors, and the two quadrature
//! intrinsics — is rejected with an explicit `not yet supported: <name>`
//! evaluation error rather than a wrong answer.
//!
//! # Design: a data-driven registry
//!
//! The Java file is a 226-arm `switch`. Here the dispatch is a table
//! ([`INTRINSICS`]) of `name -> (arity, body)`. Adding the remaining ~186 arms
//! is purely additive: append a row. Two body shapes exist:
//!
//! * [`Body::Strict`] — arguments are evaluated to `f64` first (the common case).
//! * [`Body::Lazy`] — the intrinsic sees the raw AST plus the environment, which
//!   is what `if` (evaluate only the taken branch), `sum`/`product` (bind the
//!   index variable) and the string intrinsics need.
//!
//! # Angles
//!
//! The Java engine is **radian-only**. `Math.sin/cos/tan/asin/acos/atan/atan2`
//! are used unwrapped, there is no degree/radian mode anywhere in
//! `Evaluator.java`, and `FunctionRegistry` documents every inverse-trig result
//! as `[rad]`. `angledeg` is the single degree-producing intrinsic and it does
//! the `* 180 / pi` conversion itself. This port matches: radians everywhere.
//!
//! # Determinism
//!
//! All transcendentals go through [`libm`] so a native run and a
//! `wasm32-unknown-unknown` run agree bit for bit; the host `f64` intrinsics
//! are deliberately not used.

use crate::ast::{BinOp, CmpOp, Expr, LogicOp};
use crate::diag::{FreesError, Result};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Variable bindings visible to an evaluation.
pub type Scope = HashMap<String, f64>;

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

/// The variable environment an expression is evaluated against.
///
/// The Java evaluator mutates the caller's `Map` to bind a `sum`/`product`
/// index and restores it in a `finally` block. Taking `&Scope` instead, this
/// port models the binding as a cons-cell chain: an inner binding shadows the
/// outer scope, costs no allocation, and cannot leak on an error path.
#[derive(Debug)]
pub enum Env<'a> {
    /// The caller's scope — the solver's current iterate.
    Root(&'a Scope),
    /// A single shadowing binding (a `sum`/`product` index variable).
    Bind {
        name: &'a str,
        value: f64,
        parent: &'a Env<'a>,
    },
}

impl Env<'_> {
    /// Innermost binding of `name`, if any. `name` is expected lowercase.
    pub fn get(&self, name: &str) -> Option<f64> {
        let mut cursor = self;
        loop {
            match cursor {
                Env::Root(scope) => return scope.get(name).copied(),
                Env::Bind {
                    name: bound,
                    value,
                    parent,
                } => {
                    if *bound == name {
                        return Some(*value);
                    }
                    cursor = parent;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in constants (`ConstantsRegistry.java`)
// ---------------------------------------------------------------------------

/// A `#`-suffixed built-in constant.
///
/// Port of `ConstantsRegistry.Constant`. The Java parser substitutes these as
/// numeric literals at parse time; the evaluator resolves them as a fallback so
/// a hand-built AST behaves the same.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuiltinConstant {
    /// Canonical display name, e.g. `R#`.
    pub name: &'static str,
    /// Value in SI.
    pub value: f64,
    /// SI unit string; `None` for dimensionless.
    pub unit: Option<&'static str>,
    pub description: &'static str,
}

/// The full `ConstantsRegistry` table, in declaration order.
pub const CONSTANTS: &[BuiltinConstant] = &[
    // Mathematical
    BuiltinConstant {
        name: "pi#",
        value: std::f64::consts::PI,
        unit: None,
        description: "Ratio of a circle's circumference to its diameter",
    },
    BuiltinConstant {
        name: "e#",
        value: std::f64::consts::E,
        unit: None,
        description: "Euler's number (base of the natural logarithm)",
    },
    // Universal physical constants (CODATA / SI exact where applicable)
    BuiltinConstant {
        name: "R#",
        value: 8.314_462_618,
        unit: Some("J/mol-K"),
        description: "Universal (molar) gas constant",
    },
    BuiltinConstant {
        name: "g#",
        value: 9.806_65,
        unit: Some("m/s^2"),
        description: "Standard acceleration of gravity",
    },
    BuiltinConstant {
        name: "Na#",
        value: 6.022_140_76e23,
        unit: Some("1/mol"),
        description: "Avogadro constant",
    },
    BuiltinConstant {
        name: "k#",
        value: 1.380_649e-23,
        unit: Some("J/K"),
        description: "Boltzmann constant",
    },
    BuiltinConstant {
        name: "h#",
        value: 6.626_070_15e-34,
        unit: Some("J-s"),
        description: "Planck constant",
    },
    BuiltinConstant {
        name: "c#",
        value: 299_792_458.0,
        unit: Some("m/s"),
        description: "Speed of light in vacuum",
    },
    BuiltinConstant {
        name: "sigma#",
        value: 5.670_374_419e-8,
        unit: Some("W/m^2-K^4"),
        description: "Stefan-Boltzmann constant",
    },
    BuiltinConstant {
        name: "Gc#",
        value: 6.674_30e-11,
        unit: Some("m^3/kg-s^2"),
        description: "Newtonian constant of gravitation",
    },
    BuiltinConstant {
        name: "qe#",
        value: 1.602_176_634e-19,
        unit: Some("Coulomb"),
        description: "Elementary charge",
    },
];

/// The built-in constant for `name`, matched case-insensitively.
///
/// Port of `ConstantsRegistry.lookup`.
pub fn lookup_constant(name: &str) -> Option<&'static BuiltinConstant> {
    CONSTANTS.iter().find(|c| c.name.eq_ignore_ascii_case(name))
}

// ---------------------------------------------------------------------------
// Intrinsic registry
// ---------------------------------------------------------------------------

/// How many arguments an intrinsic accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arity {
    Exact(usize),
    /// Inclusive `[min, max]`.
    Range(usize, usize),
    /// One of a fixed set (`if` takes 3 or 5).
    OneOf(&'static [usize]),
    AtLeast(usize),
    /// Variadic, zero allowed.
    Any,
}

impl Arity {
    fn accepts(self, n: usize) -> bool {
        match self {
            Arity::Exact(k) => n == k,
            Arity::Range(lo, hi) => n >= lo && n <= hi,
            Arity::OneOf(set) => set.contains(&n),
            Arity::AtLeast(lo) => n >= lo,
            Arity::Any => true,
        }
    }

    fn describe(self) -> String {
        match self {
            Arity::Exact(1) => "1 argument".to_string(),
            Arity::Exact(k) => format!("{k} arguments"),
            Arity::Range(lo, hi) => format!("{lo} to {hi} arguments"),
            Arity::OneOf(set) => {
                let names: Vec<String> = set.iter().map(|k| k.to_string()).collect();
                format!("{} arguments", names.join(" or "))
            }
            Arity::AtLeast(lo) => format!("at least {lo} argument(s)"),
            Arity::Any => "any number of arguments".to_string(),
        }
    }
}

/// An intrinsic whose arguments are all evaluated to scalars first.
pub type StrictFn = fn(&str, &[f64]) -> Result<f64>;

/// An intrinsic that needs the unevaluated AST — lazy branches, bound indices,
/// string-literal arguments.
pub type LazyFn = for<'a> fn(&str, &'a [Expr], &'a Env<'a>) -> Result<f64>;

/// The two shapes an intrinsic body can take.
#[derive(Clone, Copy)]
pub enum Body {
    Strict(StrictFn),
    Lazy(LazyFn),
}

impl std::fmt::Debug for Body {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Body::Strict(_) => f.write_str("Strict(..)"),
            Body::Lazy(_) => f.write_str("Lazy(..)"),
        }
    }
}

/// One row of the dispatch table.
#[derive(Debug, Clone, Copy)]
pub struct Intrinsic {
    /// Lowercase name, exactly as `Expr::Call` stores it.
    pub name: &'static str,
    pub arity: Arity,
    pub body: Body,
}

macro_rules! strict {
    ($name:literal, $arity:expr, $f:expr) => {
        Intrinsic {
            name: $name,
            arity: $arity,
            body: Body::Strict($f),
        }
    };
}

macro_rules! lazy {
    ($name:literal, $arity:expr, $f:expr) => {
        Intrinsic {
            name: $name,
            arity: $arity,
            body: Body::Lazy($f),
        }
    };
}

/// The dispatch table. Append rows to grow it; order is irrelevant.
pub const INTRINSICS: &[Intrinsic] = &[
    // ----- elementary algebra --------------------------------------------
    strict!("abs", Arity::Exact(1), |_, a| Ok(libm::fabs(a[0]))),
    strict!("sign", Arity::Exact(1), |_, a| Ok(java_signum(a[0]))),
    strict!("sqrt", Arity::Exact(1), |n, a| {
        if a[0] < 0.0 {
            return Err(domain(
                n,
                format_args!("square root of a negative number ({})", a[0]),
            ));
        }
        Ok(libm::sqrt(a[0]))
    }),
    strict!("cbrt", Arity::Exact(1), |_, a| Ok(libm::cbrt(a[0]))),
    strict!("sqr", Arity::Exact(1), |_, a| Ok(a[0] * a[0])),
    strict!("cube", Arity::Exact(1), |_, a| Ok(a[0] * a[0] * a[0])),
    strict!("hypot", Arity::Exact(2), |_, a| Ok(libm::hypot(a[0], a[1]))),
    strict!("exp", Arity::Exact(1), |_, a| Ok(libm::exp(a[0]))),
    strict!("ln", Arity::Exact(1), |n, a| check_log(n, a[0])
        .map(libm::log)),
    strict!("log10", Arity::Exact(1), |n, a| check_log(n, a[0])
        .map(libm::log10)),
    // The Java arm is `Math.log(x) / Math.log(2.0)`, not a dedicated log2.
    strict!("log2", Arity::Exact(1), |n, a| {
        check_log(n, a[0]).map(|x| libm::log(x) / libm::log(2.0))
    }),
    // `log` is not a Java arm; frees/EES spell base-10 as `log10`. Kept as an
    // alias so the classic spelling resolves instead of "unknown function".
    strict!("log", Arity::Exact(1), |n, a| check_log(n, a[0])
        .map(libm::log10)),
    // ----- rounding & integer parts ---------------------------------------
    strict!("floor", Arity::Exact(1), |_, a| Ok(libm::floor(a[0]))),
    strict!("ceil", Arity::Exact(1), |_, a| Ok(libm::ceil(a[0]))),
    strict!("trunc", Arity::Exact(1), |_, a| Ok(libm::trunc(a[0]))),
    strict!("int", Arity::Exact(1), |_, a| Ok(libm::trunc(a[0]))),
    strict!("round", Arity::Range(1, 2), |_, a| {
        if a.len() == 2 {
            let factor = libm::pow(10.0, java_round(a[1]));
            return Ok(java_round(a[0] * factor) / factor);
        }
        Ok(java_round(a[0]))
    }),
    // ----- trigonometry (radians) -----------------------------------------
    strict!("sin", Arity::Exact(1), |_, a| Ok(libm::sin(a[0]))),
    strict!("cos", Arity::Exact(1), |_, a| Ok(libm::cos(a[0]))),
    strict!("tan", Arity::Exact(1), |_, a| Ok(libm::tan(a[0]))),
    strict!("asin", Arity::Exact(1), |n, a| check_unit_interval(n, a[0])
        .map(libm::asin)),
    strict!("arcsin", Arity::Exact(1), |n, a| check_unit_interval(
        n, a[0]
    )
    .map(libm::asin)),
    strict!("acos", Arity::Exact(1), |n, a| check_unit_interval(n, a[0])
        .map(libm::acos)),
    strict!("arccos", Arity::Exact(1), |n, a| check_unit_interval(
        n, a[0]
    )
    .map(libm::acos)),
    strict!("atan", Arity::Exact(1), |_, a| Ok(libm::atan(a[0]))),
    strict!("arctan", Arity::Exact(1), |_, a| Ok(libm::atan(a[0]))),
    strict!("atan2", Arity::Exact(2), |_, a| Ok(libm::atan2(a[0], a[1]))),
    // ----- hyperbolic ------------------------------------------------------
    strict!("sinh", Arity::Exact(1), |_, a| Ok(libm::sinh(a[0]))),
    strict!("cosh", Arity::Exact(1), |_, a| Ok(libm::cosh(a[0]))),
    strict!("tanh", Arity::Exact(1), |_, a| Ok(libm::tanh(a[0]))),
    // Java spells the inverses out as logarithms rather than calling Math, so
    // the same closed forms are used here for bit-level agreement.
    strict!("arcsinh", Arity::Exact(1), |_, a| {
        Ok(libm::log(a[0] + libm::sqrt(a[0] * a[0] + 1.0)))
    }),
    strict!("arccosh", Arity::Exact(1), |n, a| {
        if a[0] < 1.0 {
            return Err(domain(
                n,
                format_args!("argument must be >= 1.0, got {}", a[0]),
            ));
        }
        Ok(libm::log(a[0] + libm::sqrt(a[0] * a[0] - 1.0)))
    }),
    strict!("arctanh", Arity::Exact(1), |n, a| {
        if libm::fabs(a[0]) >= 1.0 {
            return Err(domain(
                n,
                format_args!("argument must be in (-1, 1), got {}", a[0]),
            ));
        }
        Ok(0.5 * libm::log((1.0 + a[0]) / (1.0 - a[0])))
    }),
    // ----- piecewise -------------------------------------------------------
    strict!("step", Arity::Exact(1), |_, a| Ok(if a[0] >= 0.0 {
        1.0
    } else {
        0.0
    })),
    // `ramp` has no Java arm; defined as the integral of `step` so the two
    // agree at the origin (`step(0) = 1`, `ramp(0) = 0`).
    strict!("ramp", Arity::Exact(1), |_, a| Ok(if a[0] >= 0.0 {
        a[0]
    } else {
        0.0
    })),
    // ----- reductions over the argument list --------------------------------
    strict!("min", Arity::AtLeast(1), |_, a| {
        Ok(a.iter().copied().fold(f64::INFINITY, java_min))
    }),
    strict!("max", Arity::AtLeast(1), |_, a| {
        Ok(a.iter().copied().fold(f64::NEG_INFINITY, java_max))
    }),
    // ----- number theory & bitwise -----------------------------------------
    strict!("mod", Arity::Exact(2), |n, a| java_mod(n, a[0], a[1])),
    strict!("rem", Arity::Exact(2), |n, a| java_mod(n, a[0], a[1])),
    strict!("gcd", Arity::Exact(2), |_, a| Ok(
        gcd_i64(a[0] as i64, a[1] as i64) as f64
    )),
    strict!("lcm", Arity::Exact(2), |n, a| {
        let (x, y) = (a[0] as i64, a[1] as i64);
        let g = gcd_i64(x, y);
        if g == 0 {
            return Ok(0.0);
        }
        match (x / g).checked_mul(y) {
            Some(v) => Ok(v.unsigned_abs() as f64),
            None => Err(domain(n, format_args!("overflow computing lcm({x}, {y})"))),
        }
    }),
    strict!("bitand", Arity::Exact(2), |_, a| Ok(
        ((a[0] as i64) & (a[1] as i64)) as f64
    )),
    strict!("bitor", Arity::Exact(2), |_, a| Ok(
        ((a[0] as i64) | (a[1] as i64)) as f64
    )),
    strict!("bitxor", Arity::Exact(2), |_, a| Ok(
        ((a[0] as i64) ^ (a[1] as i64)) as f64
    )),
    strict!("bitnot", Arity::Exact(1), |_, a| Ok(!(a[0] as i64) as f64)),
    strict!("bitshiftl", Arity::Exact(2), |_, a| Ok(
        (a[0] as i64).wrapping_shl(a[1] as i32 as u32) as f64
    )),
    strict!("bitshiftr", Arity::Exact(2), |_, a| Ok(
        (a[0] as i64).wrapping_shr(a[1] as i32 as u32) as f64
    )),
    // ----- special functions ------------------------------------------------
    strict!("erf", Arity::Exact(1), |_, a| Ok(libm::erf(a[0]))),
    strict!("erfc", Arity::Exact(1), |_, a| Ok(libm::erfc(a[0]))),
    strict!("gamma", Arity::Exact(1), |n, a| gamma_checked(n, a[0])),
    // Apache's `Gamma.logGamma` — the Java arm — returns NaN for *every*
    // `x <= 0`, not just the poles: it is `log Γ(x)`, undefined wherever Γ is
    // negative. `libm::lgamma` is the different function `log |Γ(x)|`, which
    // happily returns 1.2655… for `loggamma(-0.5)`. Refusing the whole
    // non-positive half-line keeps the two in step (this port raises where Java
    // yields NaN, as elsewhere) instead of inventing a value Java never returns.
    strict!("loggamma", Arity::Exact(1), |n, a| {
        if a[0] <= 0.0 {
            return Err(domain(
                n,
                format_args!("argument must be > 0, got {}", a[0]),
            ));
        }
        Ok(libm::lgamma(a[0]))
    }),
    strict!("factorial", Arity::Exact(1), |n, a| {
        if a[0] <= -1.0 {
            return Err(domain(
                n,
                format_args!("argument must be > -1, got {}", a[0]),
            ));
        }
        gamma_checked(n, a[0] + 1.0)
    }),
    strict!("beta", Arity::Exact(2), |n, a| {
        if a[0] <= 0.0 || a[1] <= 0.0 {
            return Err(domain(
                n,
                format_args!("both arguments must be > 0, got ({}, {})", a[0], a[1]),
            ));
        }
        Ok(libm::exp(
            libm::lgamma(a[0]) + libm::lgamma(a[1]) - libm::lgamma(a[0] + a[1]),
        ))
    }),
    // ----- orthogonal polynomials (three-term recurrences) ------------------
    strict!("legendrep", Arity::Exact(2), |n, a| poly_order(n, a[0])
        .map(|k| legendre_p(k, a[1]))),
    strict!("chebyshevt", Arity::Exact(2), |n, a| poly_order(n, a[0])
        .map(|k| chebyshev_t(k, a[1]))),
    strict!("chebyshevu", Arity::Exact(2), |n, a| poly_order(n, a[0])
        .map(|k| chebyshev_u(k, a[1]))),
    strict!("hermiteh", Arity::Exact(2), |n, a| poly_order(n, a[0])
        .map(|k| hermite_h(k, a[1]))),
    strict!("laguerrel", Arity::Exact(2), |n, a| poly_order(n, a[0])
        .map(|k| laguerre_l(k, a[1]))),
    // ----- descriptive statistics -------------------------------------------
    strict!("mean", Arity::AtLeast(1), |_, a| Ok(mean(a))),
    strict!("median", Arity::AtLeast(1), |_, a| Ok(median(a))),
    strict!("variance", Arity::AtLeast(1), |_, a| Ok(sample_variance(a))),
    strict!("var", Arity::AtLeast(1), |_, a| Ok(sample_variance(a))),
    strict!("stdev", Arity::AtLeast(1), |_, a| Ok(libm::sqrt(
        sample_variance(a)
    ))),
    strict!("stddev", Arity::AtLeast(1), |_, a| Ok(libm::sqrt(
        sample_variance(a)
    ))),
    strict!("std", Arity::AtLeast(1), |_, a| Ok(libm::sqrt(
        sample_variance(a)
    ))),
    strict!("rms", Arity::AtLeast(1), |_, a| {
        Ok(libm::sqrt(
            a.iter().map(|x| x * x).sum::<f64>() / a.len() as f64,
        ))
    }),
    // Java's `average` yields 0.0 for an empty list (`.average().orElse(0.0)`).
    strict!("average", Arity::Any, |_, a| Ok(if a.is_empty() {
        0.0
    } else {
        mean(a)
    })),
    strict!("avg", Arity::Any, |_, a| Ok(if a.is_empty() {
        0.0
    } else {
        mean(a)
    })),
    strict!("percentile", Arity::AtLeast(2), |n, a| percentile(
        n,
        a[0],
        &a[1..]
    )),
    // ----- normal distribution (erf-based, matching Apache's formulas) -------
    strict!("normalcdf", Arity::Range(1, 3), |n, a| {
        let (mu, sigma) = normal_params(n, a)?;
        Ok(0.5 * libm::erfc(-(a[0] - mu) / (sigma * std::f64::consts::SQRT_2)))
    }),
    strict!("normalpdf", Arity::Range(1, 3), |n, a| {
        let (mu, sigma) = normal_params(n, a)?;
        let z = (a[0] - mu) / sigma;
        Ok(libm::exp(-0.5 * z * z) / (sigma * libm::sqrt(2.0 * std::f64::consts::PI)))
    }),
    strict!("probability", Arity::Exact(4), |n, a| {
        let (x1, x2, mu, sigma) = (a[0], a[1], a[2], a[3]);
        if sigma <= 0.0 {
            return Err(domain(
                n,
                format_args!("standard deviation must be > 0, got {sigma}"),
            ));
        }
        let d = sigma * std::f64::consts::SQRT_2;
        Ok(0.5 * (libm::erf((x2 - mu) / d) - libm::erf((x1 - mu) / d)))
    }),
    // ----- degenerate complex helpers (a real scalar is z with Im(z) = 0) ----
    strict!("real", Arity::Exact(1), |_, a| Ok(a[0])),
    strict!("imag", Arity::Exact(1), |_, _a| Ok(0.0)),
    strict!("conj", Arity::Exact(1), |_, a| Ok(a[0])),
    strict!("magnitude", Arity::Exact(1), |_, a| Ok(libm::fabs(a[0]))),
    strict!("angle", Arity::Exact(1), |_, a| Ok(libm::atan2(0.0, a[0]))),
    strict!("anglerad", Arity::Exact(1), |_, a| Ok(libm::atan2(
        0.0, a[0]
    ))),
    strict!("angledeg", Arity::Exact(1), |_, a| {
        Ok(libm::atan2(0.0, a[0]) * 180.0 / std::f64::consts::PI)
    }),
    strict!("cis", Arity::Exact(1), |_, a| Ok(libm::cos(a[0]))),
    // `pi()` is not a Java arm — the constant is spelled `pi#`. Kept as a
    // zero-argument convenience so both spellings resolve.
    strict!("pi", Arity::Exact(0), |_, _a| Ok(std::f64::consts::PI)),
    // ----- lazy forms --------------------------------------------------------
    lazy!("if", Arity::OneOf(&[3, 5]), eval_if),
    lazy!("sum", Arity::Any, |n, args, env| eval_reduction(
        n,
        args,
        env,
        0.0,
        |a, b| a + b
    )),
    lazy!("product", Arity::Any, |n, args, env| eval_reduction(
        n,
        args,
        env,
        1.0,
        |a, b| a * b
    )),
    lazy!("stringlen", Arity::Exact(1), |n, args, _| Ok(
        string_arg(n, &args[0])?.chars().count() as f64
    )),
    lazy!("stringval", Arity::Exact(1), |n, args, _| {
        let s = string_arg(n, &args[0])?;
        s.trim()
            .parse::<f64>()
            .map_err(|_| domain(n, format_args!("'{s}' is not a number")))
    }),
    lazy!("stringpos", Arity::Exact(2), |n, args, _| {
        let hay = string_arg(n, &args[0])?;
        let needle = string_arg(n, &args[1])?;
        // Java returns a 1-based index, 0 when absent.
        Ok(match hay.find(&needle) {
            Some(byte_index) => hay[..byte_index].chars().count() as f64 + 1.0,
            None => 0.0,
        })
    }),
];

fn registry() -> &'static HashMap<&'static str, &'static Intrinsic> {
    static REGISTRY: OnceLock<HashMap<&'static str, &'static Intrinsic>> = OnceLock::new();
    REGISTRY.get_or_init(|| INTRINSICS.iter().map(|i| (i.name, i)).collect())
}

/// The intrinsic registered under `name` (already lowercase), if any.
pub fn lookup_intrinsic(name: &str) -> Option<&'static Intrinsic> {
    registry().get(name).copied()
}

// ---------------------------------------------------------------------------
// Known-but-unported intrinsic families
// ---------------------------------------------------------------------------

/// `(name, family)` for Java arms this pass deliberately does not implement.
///
/// Reporting them as *not yet supported* rather than *unknown function* keeps
/// the distinction the parent engine cares about: a refusal is honest, a wrong
/// answer is not.
const UNPORTED: &[(&str, &str)] = &[
    // Calculus
    ("integral", "calculus"),
    ("gaussintegral", "calculus"),
    ("uncertaintyof", "uncertainty propagation"),
    // TABLE lookup / interpolation
    ("interpolate", "table lookup"),
    ("interpolate1", "table lookup"),
    ("interpolate2d", "table lookup"),
    ("lookup", "table lookup"),
    ("lookuprow", "table lookup"),
    ("nlookuprows", "table lookup"),
    ("differentiate", "table lookup"),
    ("differentiate1", "table lookup"),
    ("dtable", "table lookup"),
    ("dtable1", "table lookup"),
    // Parametric-table accessors
    ("tablerun#", "parametric table"),
    ("tablerun", "parametric table"),
    ("nparametricruns", "parametric table"),
    ("tablevalue", "parametric table"),
    ("tablesum", "parametric table"),
    ("tableavg", "parametric table"),
    ("tablemin", "parametric table"),
    ("tablemax", "parametric table"),
    ("tablestddev", "parametric table"),
    ("integralvalue", "parametric table"),
    // DYNAMIC / ODE result accessors
    ("odevalue", "ODE results"),
    ("finalvalue", "ODE results"),
    ("maxvalue", "ODE results"),
    ("minvalue", "ODE results"),
    ("timeat", "ODE results"),
    ("odeavg", "ODE results"),
    ("odesum", "ODE results"),
    ("odestddev", "ODE results"),
    ("odemin", "ODE results"),
    ("odemax", "ODE results"),
    // Arrays / matrices
    ("arrayelmt", "arrays"),
    ("inv", "matrices"),
    ("det", "matrices"),
    ("trace", "matrices"),
    ("transpose", "matrices"),
    ("eig", "matrices"),
    ("eigvec", "matrices"),
    ("rank", "matrices"),
    ("norm", "matrices"),
    ("cond", "matrices"),
    ("svd", "matrices"),
    ("qr", "matrices"),
    ("cholesky", "matrices"),
    ("matexp", "matrices"),
    // Control systems
    ("tf", "control systems"),
    ("ss", "control systems"),
    ("tf2ss", "control systems"),
    ("ss2tf", "control systems"),
    ("series", "control systems"),
    ("parallel", "control systems"),
    ("feedback", "control systems"),
    ("impulse", "control systems"),
    ("lsim", "control systems"),
    ("bode", "control systems"),
    ("nyquist", "control systems"),
    ("nichols", "control systems"),
    ("margin", "control systems"),
    ("stepinfo", "control systems"),
    ("c2d", "control systems"),
    ("d2c", "control systems"),
    ("rlocus", "control systems"),
    ("routh", "control systems"),
    ("pade", "control systems"),
    ("lqr", "control systems"),
    ("dlqr", "control systems"),
    ("dare", "control systems"),
    ("lyap", "control systems"),
    ("dlyap", "control systems"),
    ("ctrb", "control systems"),
    ("obsv", "control systems"),
    ("place", "control systems"),
    ("acker", "control systems"),
    ("lqe", "control systems"),
    ("gram", "control systems"),
    ("balreal", "control systems"),
    ("pidtune", "control systems"),
    // Special functions needing kernels libm does not provide
    ("erfinv", "special functions"),
    ("digamma", "special functions"),
    ("normalinvcdf", "special functions"),
    ("chi_square", "special functions"),
    ("besselj", "Bessel functions"),
    ("bessely", "Bessel functions"),
    ("besseli", "Bessel functions"),
    ("besselk", "Bessel functions"),
    ("besselj0", "Bessel functions"),
    ("besselj1", "Bessel functions"),
    ("bessely0", "Bessel functions"),
    ("bessely1", "Bessel functions"),
    ("besseli0", "Bessel functions"),
    ("besseli1", "Bessel functions"),
    ("besselk0", "Bessel functions"),
    ("besselk1", "Bessel functions"),
    // Non-deterministic
    ("random", "random numbers"),
    ("randg", "random numbers"),
    // Regression & conversion
    ("slope", "regression"),
    ("intercept", "regression"),
    ("r2", "regression"),
    ("baseconvert", "string/number conversion"),
    // Engineering correlations & property backends
    ("stagnationtemp", "compressible flow"),
    ("stagnationpres", "compressible flow"),
    ("prandtlmeyer", "compressible flow"),
    ("prandtl_meyer", "compressible flow"),
    ("machangle", "compressible flow"),
    ("mach_a_astar", "compressible flow"),
    ("mach_prandtlmeyer", "compressible flow"),
    ("mach_shock", "compressible flow"),
    ("m2_shock", "compressible flow"),
    ("p2_p1_shock", "compressible flow"),
    ("t2_t1_shock", "compressible flow"),
    ("rho2_rho1_shock", "compressible flow"),
    ("p02_p01_shock", "compressible flow"),
    ("t0_t", "compressible flow"),
    ("p0_p", "compressible flow"),
    ("rho0_rho", "compressible flow"),
    ("a_astar", "compressible flow"),
    ("theta_oblique", "compressible flow"),
    ("beta_oblique", "compressible flow"),
    ("lmtd", "heat transfer"),
    ("fin_efficiency", "heat transfer"),
    ("heisler_temp", "heat transfer"),
    ("heisler_q", "heat transfer"),
    ("viewfactor_perp", "radiation"),
    ("viewfactor_plates", "radiation"),
    ("viewfactor_disks", "radiation"),
    ("friction_factor", "flow networks"),
    ("darcy_friction", "flow networks"),
    ("reynolds", "flow networks"),
    ("re_number", "flow networks"),
    ("minor_loss", "flow networks"),
    ("iso6358", "pneumatics"),
    ("isa_t", "standard atmosphere"),
    ("isa_p", "standard atmosphere"),
    ("isa_rho", "standard atmosphere"),
    ("eos_z", "cubic EOS"),
    ("eos_volume", "cubic EOS"),
    ("eos_density", "cubic EOS"),
    ("eos_pressure", "cubic EOS"),
    ("eos_enthalpy", "cubic EOS"),
    ("eos_entropy", "cubic EOS"),
    ("eos_psat", "cubic EOS"),
    ("adiabaticflametemp", "combustion"),
    ("adiabaticflametemperature", "combustion"),
    ("adiabaticflametempeq", "combustion"),
    ("flametemp", "combustion"),
    ("flametemp_eq", "combustion"),
    ("wiebe", "combustion"),
    ("wiebe_rate", "combustion"),
    ("zone_ramp", "two-phase flow"),
    ("friedel_phi2", "two-phase flow"),
    ("momentum_flux", "two-phase flow"),
    ("mass_flux", "heat transfer"),
    ("j_fin", "heat transfer"),
    ("f_fin", "heat transfer"),
    ("ua_hx", "heat transfer"),
];

/// The family `name` belongs to, if it is a known-but-unported Java arm.
fn unported_family(name: &str) -> Option<&'static str> {
    // Every synthetic call the Java parser generates carries a `$` (prop$…,
    // proc$…, eigen$…, series$…). One rule covers all of them.
    if name.contains('$') {
        return Some("synthetic property / procedure / matrix / control-systems call");
    }
    // Prefixed correlation families, which would otherwise need ~120 more rows.
    for prefix in [
        "htc_",
        "dp_",
        "nu_",
        "hx_",
        "void_",
        "lm_",
        "mix_",
        "eq_",
        "isen_",
        "fanno_",
        "rayleigh_",
        "chen_",
        "bessel_",
    ] {
        if name.starts_with(prefix) {
            return Some("engineering correlation library");
        }
    }
    UNPORTED
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, family)| *family)
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/// Evaluate `expr` to a scalar under `scope`.
///
/// Unknown variables are an [`crate::diag::FreesError::Evaluation`]; the solver
/// always supplies a full scope for the block it is iterating.
pub fn eval(expr: &Expr, scope: &Scope) -> Result<f64> {
    let env = Env::Root(scope);
    eval_in(expr, &env)
}

/// Evaluate `expr` against an arbitrary [`Env`] — the entry point the binding
/// intrinsics recurse through.
pub fn eval_in<'a>(expr: &'a Expr, env: &'a Env<'a>) -> Result<f64> {
    match expr {
        // Literals are already SI: the parser folded `value * factor + offset`.
        // `is_imaginary` is ignored here exactly as the Java arm ignores it;
        // complex handling happens in the (unported) ComplexExpansion pass.
        Expr::Num { value, .. } => Ok(*value),

        Expr::Str(value) => Err(FreesError::evaluation(format!(
            "string literal '{value}' cannot be evaluated as a number; \
             string literals are only valid as function arguments"
        ))),

        Expr::Var(name) => match env.get(name) {
            Some(value) => Ok(value),
            // `#`-suffixed built-ins are normally substituted at parse time;
            // resolve them here too so a hand-built AST behaves identically.
            None => match lookup_constant(name) {
                Some(constant) => Ok(constant.value),
                None => Err(FreesError::evaluation(format!(
                    "variable has no value: {name}"
                ))),
            },
        },

        Expr::Neg(operand) => Ok(-eval_in(operand, env)?),

        Expr::BinOp { op, left, right } => {
            let l = eval_in(left, env)?;
            let r = eval_in(right, env)?;
            apply_binop(*op, l, r)
        }

        Expr::Compare { op, left, right } => {
            let l = eval_in(left, env)?;
            let r = eval_in(right, env)?;
            let truth = match op {
                CmpOp::Lt => l < r,
                CmpOp::Gt => l > r,
                CmpOp::Le => l <= r,
                CmpOp::Ge => l >= r,
                CmpOp::Ne => l != r,
                CmpOp::Eq => l == r,
            };
            Ok(if truth { 1.0 } else { 0.0 })
        }

        // Java evaluates *both* operands before combining — no short-circuit.
        // Preserved so evaluation order (and therefore which error surfaces
        // first) matches the reference engine.
        Expr::Logical { op, left, right } => {
            let l = eval_in(left, env)?;
            let r = eval_in(right, env)?;
            let truth = match op {
                LogicOp::And => l != 0.0 && r != 0.0,
                LogicOp::Or => l != 0.0 || r != 0.0,
            };
            Ok(if truth { 1.0 } else { 0.0 })
        }

        Expr::Not(operand) => Ok(if eval_in(operand, env)? == 0.0 {
            1.0
        } else {
            0.0
        }),

        Expr::Call { function, args } => eval_call(function, args, env),

        // The parser rewrites `a[i]` into the scalar variable it names before a
        // solve; reaching the evaluator means that rewrite has not happened.
        Expr::ArrayAccess { name, indices } => Err(FreesError::evaluation(format!(
            "array access `{name}[…]` cannot be evaluated directly \
             ({} index expression(s)); it must be expanded to scalars first",
            indices.len()
        ))),

        Expr::Range { .. } => Err(FreesError::evaluation(
            "an index range `a:b` cannot be evaluated as a scalar".to_string(),
        )),

        Expr::ArrayLiteral(elements) => Err(FreesError::evaluation(format!(
            "an array literal ({} element(s)) cannot be evaluated as a scalar",
            elements.len()
        ))),
    }
}

fn apply_binop(op: BinOp, l: f64, r: f64) -> Result<f64> {
    // The four element-wise operators degenerate to their scalar forms.
    let op = op.scalar_equivalent();
    match op {
        BinOp::Add => Ok(l + r),
        BinOp::Sub => Ok(l - r),
        BinOp::Mul => Ok(l * r),
        BinOp::Div => {
            if r == 0.0 {
                return Err(FreesError::evaluation(format!("division by zero: {l} / 0")));
            }
            Ok(l / r)
        }
        // `a \ b` is `b / a` for scalars (A⁻¹B for matrices).
        BinOp::LeftDiv => {
            if l == 0.0 {
                return Err(FreesError::evaluation(format!(
                    "division by zero: 0 \\ {r} (left division divides by the left operand)"
                )));
            }
            Ok(r / l)
        }
        BinOp::Pow => {
            if l < 0.0 && r != libm::floor(r) {
                return Err(FreesError::evaluation(format!(
                    "negative base raised to a non-integer power: {l} ^ {r}"
                )));
            }
            if l == 0.0 && r < 0.0 {
                return Err(FreesError::evaluation(format!(
                    "division by zero: 0 raised to the negative power {r}"
                )));
            }
            Ok(libm::pow(l, r))
        }
        // `scalar_equivalent` maps every element-wise operator onto one of the
        // arms above, so nothing reaches here.
        BinOp::ElemMul | BinOp::ElemDiv | BinOp::ElemLeftDiv | BinOp::ElemPow => Err(
            FreesError::evaluation(format!("unhandled operator: {}", op.as_str())),
        ),
    }
}

fn eval_call<'a>(function: &str, args: &'a [Expr], env: &'a Env<'a>) -> Result<f64> {
    let Some(intrinsic) = lookup_intrinsic(function) else {
        return Err(match unported_family(function) {
            Some(family) => {
                FreesError::evaluation(format!("not yet supported: {function} ({family})"))
            }
            None => FreesError::evaluation(format!("unknown function: {function}")),
        });
    };

    if !intrinsic.arity.accepts(args.len()) {
        return Err(FreesError::evaluation(format!(
            "{function} expects {}, got {}",
            intrinsic.arity.describe(),
            args.len()
        )));
    }

    match intrinsic.body {
        Body::Lazy(f) => f(function, args, env),
        Body::Strict(f) => {
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                values.push(eval_in(arg, env)?);
            }
            f(function, &values)
        }
    }
}

// ---------------------------------------------------------------------------
// Lazy intrinsic bodies
// ---------------------------------------------------------------------------

/// `If(a, b, lt, eq, gt)` — the frees/EES five-argument form — plus the
/// three-argument `if(cond, then, else)` shorthand. Only the taken branch is
/// evaluated in either form.
fn eval_if<'a>(name: &str, args: &'a [Expr], env: &'a Env<'a>) -> Result<f64> {
    if args.len() == 3 {
        let condition = eval_in(&args[0], env)?;
        // Same truthiness rule as `and`/`or`/`not`: non-zero is true.
        return if condition != 0.0 {
            eval_in(&args[1], env)
        } else {
            eval_in(&args[2], env)
        };
    }
    let a = eval_in(&args[0], env)?;
    let b = eval_in(&args[1], env)?;
    if a < b {
        eval_in(&args[2], env)
    } else if a == b {
        eval_in(&args[3], env)
    } else if a > b {
        eval_in(&args[4], env)
    } else {
        // Only reachable when a comparison against NaN made every branch false.
        Err(domain(
            name,
            format_args!("cannot compare {a} with {b} (not a number)"),
        ))
    }
}

/// Iterations a bounded `sum`/`product` may run before it is treated as a
/// runaway. The Java engine has no cap; in a browser an unbounded loop is a
/// hang, so this port refuses instead.
const MAX_REDUCTION_ITERATIONS: i64 = 1 << 24;

/// `sum(v, lo, hi, body)` / `product(v, lo, hi, body)` bind `v` over `body`;
/// every other shape is a plain reduction over the argument list. This mirrors
/// the special case in `Expr::variables()` — `v` never escapes as an unknown.
fn eval_reduction<'a>(
    name: &str,
    args: &'a [Expr],
    env: &'a Env<'a>,
    identity: f64,
    combine: fn(f64, f64) -> f64,
) -> Result<f64> {
    if args.len() == 4 {
        if let Expr::Var(index) = &args[0] {
            let lo = reduction_bound(name, "lower", eval_in(&args[1], env)?)?;
            let hi = reduction_bound(name, "upper", eval_in(&args[2], env)?)?;
            let span = (hi - lo).abs() + 1;
            if span > MAX_REDUCTION_ITERATIONS {
                return Err(domain(
                    name,
                    format_args!("{span} iterations exceeds the {MAX_REDUCTION_ITERATIONS} limit"),
                ));
            }
            let step = if lo <= hi { 1 } else { -1 };
            let mut acc = identity;
            let mut i = lo;
            loop {
                let inner = Env::Bind {
                    name: index.as_str(),
                    value: i as f64,
                    parent: env,
                };
                acc = combine(acc, eval_in(&args[3], &inner)?);
                if i == hi {
                    break;
                }
                i += step;
            }
            return Ok(acc);
        }
    }
    let mut acc = identity;
    for arg in args {
        acc = combine(acc, eval_in(arg, env)?);
    }
    Ok(acc)
}

/// `(int) Math.round(x)` with the silent truncation turned into an error.
fn reduction_bound(name: &str, which: &str, value: f64) -> Result<i64> {
    if !value.is_finite() {
        return Err(domain(
            name,
            format_args!("{which} bound is not a finite number ({value})"),
        ));
    }
    let rounded = java_round(value);
    if libm::fabs(rounded) > i32::MAX as f64 {
        return Err(domain(
            name,
            format_args!("{which} bound {value} is out of range"),
        ));
    }
    Ok(rounded as i64)
}

/// Java's `Evaluator.evalString`: a quoted literal, or a bare identifier
/// treated as a string label for backward compatibility.
fn string_arg(name: &str, expr: &Expr) -> Result<String> {
    match expr {
        Expr::Str(value) => Ok(value.clone()),
        Expr::Var(value) => Ok(value.clone()),
        other => Err(domain(
            name,
            format_args!("expected a string argument, got {other:?}"),
        )),
    }
}

// ---------------------------------------------------------------------------
// Numeric helpers
// ---------------------------------------------------------------------------

fn domain(function: &str, message: std::fmt::Arguments<'_>) -> FreesError {
    FreesError::evaluation(format!("{function}: {message}"))
}

fn check_log(name: &str, x: f64) -> Result<f64> {
    if x == 0.0 {
        return Err(domain(name, format_args!("logarithm of zero")));
    }
    if x < 0.0 {
        return Err(domain(
            name,
            format_args!("logarithm of a negative number ({x})"),
        ));
    }
    Ok(x)
}

fn check_unit_interval(name: &str, x: f64) -> Result<f64> {
    if !(-1.0..=1.0).contains(&x) {
        return Err(domain(
            name,
            format_args!("argument must be in [-1, 1], got {x}"),
        ));
    }
    Ok(x)
}

fn gamma_checked(name: &str, x: f64) -> Result<f64> {
    if x <= 0.0 && x == libm::floor(x) {
        return Err(domain(
            name,
            format_args!("pole at non-positive integer {x}"),
        ));
    }
    Ok(libm::tgamma(x))
}

fn normal_params(name: &str, args: &[f64]) -> Result<(f64, f64)> {
    let mu = if args.len() > 1 { args[1] } else { 0.0 };
    let sigma = if args.len() > 2 { args[2] } else { 1.0 };
    if sigma <= 0.0 {
        return Err(domain(
            name,
            format_args!("standard deviation must be positive, got {sigma}"),
        ));
    }
    Ok((mu, sigma))
}

fn poly_order(name: &str, order: f64) -> Result<usize> {
    if !order.is_finite() {
        return Err(domain(
            name,
            format_args!("polynomial order must be a finite integer, got {order}"),
        ));
    }
    let n = java_round(order);
    if n < 0.0 {
        return Err(domain(
            name,
            format_args!("polynomial order must be >= 0, got {n}"),
        ));
    }
    if n > 100_000.0 {
        return Err(domain(
            name,
            format_args!("polynomial order {n} is too large"),
        ));
    }
    Ok(n as usize)
}

/// `Math.round`: half rounds toward positive infinity (so `round(-2.5) == -2`),
/// unlike Rust's `f64::round`, which rounds half away from zero.
/// Non-finite input is returned unchanged rather than saturating to a `long`.
fn java_round(x: f64) -> f64 {
    if x.is_nan() || x.is_infinite() {
        return x;
    }
    libm::floor(x + 0.5)
}

/// `Math.signum`: `-0.0` and `NaN` are returned unchanged, unlike Rust's
/// `f64::signum`, which maps `-0.0` to `-1.0`.
fn java_signum(x: f64) -> f64 {
    if x.is_nan() {
        f64::NAN
    } else if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// `Math.min`: NaN-propagating, `-0.0 < 0.0`.
fn java_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else if b < a {
        b
    } else if a.is_sign_negative() {
        a
    } else {
        b
    }
}

/// `Math.max`: NaN-propagating, `0.0 > -0.0`.
fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else if b > a {
        b
    } else if a.is_sign_positive() {
        a
    } else {
        b
    }
}

/// Java's `%` on doubles: truncated remainder, sign of the dividend.
fn java_mod(name: &str, x: f64, y: f64) -> Result<f64> {
    if y == 0.0 {
        return Err(domain(name, format_args!("division by zero: {x} mod 0")));
    }
    Ok(libm::fmod(x, y))
}

fn gcd_i64(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a as i64
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn median(v: &[f64]) -> f64 {
    let mut sorted = v.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        0.5 * (sorted[n / 2 - 1] + sorted[n / 2])
    }
}

/// Unbiased (n−1) sample variance; `0` for a single value, as in Java.
fn sample_variance(v: &[f64]) -> f64 {
    if v.len() == 1 {
        return 0.0;
    }
    let m = mean(v);
    let ss: f64 = v.iter().map(|x| (x - m) * (x - m)).sum();
    ss / (v.len() - 1) as f64
}

/// Apache Commons `Percentile` in its default (LEGACY / R-6) estimation type,
/// which is what the Java arm uses; `p` is clamped to `[1e-9, 100]` first.
///
/// A `NaN` percentile is refused rather than clamped. `Math.min`/`Math.max`
/// propagate `NaN`, so the clamp leaves it alone and every subsequent
/// comparison answers `false` — which walked straight into `sorted[index - 1]`
/// with `index == 0` and panicked. Apache's own `Percentile` throws
/// `OutOfRangeException` for a non-finite quantile, so an error is also the
/// faithful answer. `NaN` reaches here through ordinary arithmetic
/// (`percentile(1e999 - 1e999, …)`) and through any iterate the solver pushes
/// into an invalid region, so this is a live path, not a theoretical one.
fn percentile(name: &str, p: f64, data: &[f64]) -> Result<f64> {
    if p.is_nan() {
        return Err(domain(
            name,
            format_args!("the percentile must be a number in [0, 100], got NaN"),
        ));
    }
    let p = java_max(1.0e-9, java_min(100.0, p));
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    if n <= 1 {
        // `Arity::AtLeast(2)` guarantees one data point; an empty slice can only
        // come from a direct call, and answering NaN beats indexing off the end.
        return Ok(sorted.first().copied().unwrap_or(f64::NAN));
    }
    let pos = p / 100.0 * (n as f64 + 1.0);
    if pos < 1.0 {
        return Ok(sorted[0]);
    }
    if pos >= n as f64 {
        return Ok(sorted[n - 1]);
    }
    let floor = libm::floor(pos);
    // `1 <= pos < n` and `p` is finite, so `floor` is in `[1, n)` — the clamp is
    // a no-op that makes the indices below true by construction rather than by
    // argument.
    let index = (floor as usize).clamp(1, n - 1);
    let frac = pos - floor;
    let lower = sorted[index - 1];
    let upper = sorted[index];
    Ok(lower + frac * (upper - lower))
}

/// P₀=1, P₁=x, (n+1)Pₙ₊₁=(2n+1)x·Pₙ − n·Pₙ₋₁.
fn legendre_p(n: usize, x: f64) -> f64 {
    if n == 0 {
        return 1.0;
    }
    let (mut prev, mut curr) = (1.0, x);
    for k in 1..n {
        let k = k as f64;
        let next = ((2.0 * k + 1.0) * x * curr - k * prev) / (k + 1.0);
        prev = curr;
        curr = next;
    }
    curr
}

/// T₀=1, T₁=x, Tₙ₊₁=2x·Tₙ − Tₙ₋₁.
fn chebyshev_t(n: usize, x: f64) -> f64 {
    if n == 0 {
        return 1.0;
    }
    let (mut prev, mut curr) = (1.0, x);
    for _ in 1..n {
        let next = 2.0 * x * curr - prev;
        prev = curr;
        curr = next;
    }
    curr
}

/// U₀=1, U₁=2x, Uₙ₊₁=2x·Uₙ − Uₙ₋₁.
fn chebyshev_u(n: usize, x: f64) -> f64 {
    if n == 0 {
        return 1.0;
    }
    let (mut prev, mut curr) = (1.0, 2.0 * x);
    for _ in 1..n {
        let next = 2.0 * x * curr - prev;
        prev = curr;
        curr = next;
    }
    curr
}

/// Physicists' Hermite: H₀=1, H₁=2x, Hₙ₊₁=2x·Hₙ − 2n·Hₙ₋₁.
fn hermite_h(n: usize, x: f64) -> f64 {
    if n == 0 {
        return 1.0;
    }
    let (mut prev, mut curr) = (1.0, 2.0 * x);
    for k in 1..n {
        let next = 2.0 * x * curr - 2.0 * k as f64 * prev;
        prev = curr;
        curr = next;
    }
    curr
}

/// L₀=1, L₁=1−x, (n+1)Lₙ₊₁=(2n+1−x)Lₙ − n·Lₙ₋₁.
fn laguerre_l(n: usize, x: f64) -> f64 {
    if n == 0 {
        return 1.0;
    }
    let (mut prev, mut curr) = (1.0, 1.0 - x);
    for k in 1..n {
        let k = k as f64;
        let next = ((2.0 * k + 1.0 - x) * curr - k * prev) / (k + 1.0);
        prev = curr;
        curr = next;
    }
    curr
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BinOp, CmpOp, Expr, LogicOp};

    const EPS: f64 = 1e-12;

    fn scope(pairs: &[(&str, f64)]) -> Scope {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    /// Evaluate against an empty scope, panicking on error.
    fn ev(e: &Expr) -> f64 {
        eval(e, &Scope::new()).unwrap_or_else(|err| panic!("eval failed: {err}"))
    }

    /// Evaluate expecting an error; returns the rendered message.
    fn err(e: &Expr) -> String {
        match eval(e, &Scope::new()) {
            Ok(v) => panic!("expected an error, got {v}"),
            Err(e) => e.to_string(),
        }
    }

    fn n(v: f64) -> Expr {
        Expr::num(v)
    }

    /// `f(args…)` evaluated against an empty scope.
    fn call(name: &str, args: &[f64]) -> Result<f64> {
        eval(
            &Expr::call(name, args.iter().copied().map(n).collect()),
            &Scope::new(),
        )
    }

    fn c(name: &str, args: &[f64]) -> f64 {
        call(name, args).unwrap_or_else(|e| panic!("{name} failed: {e}"))
    }

    fn cerr(name: &str, args: &[f64]) -> String {
        match call(name, args) {
            Ok(v) => panic!("expected {name} to fail, got {v}"),
            Err(e) => e.to_string(),
        }
    }

    fn close(a: f64, b: f64) {
        assert!(
            (a - b).abs() <= 1e-10 * b.abs().max(1.0),
            "expected {b}, got {a}"
        );
    }

    // -- literals, strings, variables --------------------------------------

    #[test]
    fn numeric_literals_evaluate_to_their_si_value() {
        assert_eq!(ev(&n(140_000.0)), 140_000.0);
        assert_eq!(
            ev(&Expr::Num {
                value: 3.5,
                unit: Some("Pa".into()),
                is_imaginary: false,
            }),
            3.5
        );
    }

    #[test]
    fn imaginary_literals_yield_their_magnitude_like_java() {
        // The Java arm destructures `isImaginary` and ignores it.
        assert_eq!(
            ev(&Expr::Num {
                value: 2.0,
                unit: None,
                is_imaginary: true,
            }),
            2.0
        );
    }

    #[test]
    fn string_literals_cannot_be_evaluated_numerically() {
        let message = err(&Expr::Str("water".into()));
        assert!(message.contains("string literal 'water'"), "{message}");
        assert!(
            message.contains("cannot be evaluated as a number"),
            "{message}"
        );
    }

    #[test]
    fn variables_resolve_from_the_scope() {
        let s = scope(&[("t_in", 300.0)]);
        assert_eq!(eval(&Expr::var("T_in"), &s).unwrap(), 300.0);
    }

    #[test]
    fn unknown_variables_are_an_error_not_a_zero() {
        let message = err(&Expr::var("t_in"));
        assert!(message.contains("variable has no value: t_in"), "{message}");
    }

    #[test]
    fn scope_bindings_shadow_builtin_constants() {
        let s = scope(&[("pi#", 3.0)]);
        assert_eq!(eval(&Expr::var("pi#"), &s).unwrap(), 3.0);
    }

    // -- built-in constants -------------------------------------------------

    #[test]
    fn builtin_constants_resolve_without_a_scope_entry() {
        assert_eq!(ev(&Expr::var("pi#")), std::f64::consts::PI);
        assert_eq!(ev(&Expr::var("e#")), std::f64::consts::E);
        assert_eq!(ev(&Expr::var("r#")), 8.314_462_618);
        assert_eq!(ev(&Expr::var("g#")), 9.806_65);
        assert_eq!(ev(&Expr::var("na#")), 6.022_140_76e23);
        assert_eq!(ev(&Expr::var("k#")), 1.380_649e-23);
        assert_eq!(ev(&Expr::var("h#")), 6.626_070_15e-34);
        assert_eq!(ev(&Expr::var("c#")), 299_792_458.0);
        assert_eq!(ev(&Expr::var("sigma#")), 5.670_374_419e-8);
        assert_eq!(ev(&Expr::var("gc#")), 6.674_30e-11);
        assert_eq!(ev(&Expr::var("qe#")), 1.602_176_634e-19);
    }

    #[test]
    fn the_constant_table_matches_the_java_registry_exactly() {
        // ConstantsRegistry defines eleven constants, in this order.
        let names: Vec<&str> = CONSTANTS.iter().map(|c| c.name).collect();
        assert_eq!(
            names,
            vec!["pi#", "e#", "R#", "g#", "Na#", "k#", "h#", "c#", "sigma#", "Gc#", "qe#"]
        );
        assert!(CONSTANTS.iter().all(|c| c.name.ends_with('#')));
        assert_eq!(
            lookup_constant("SIGMA#").map(|c| c.unit),
            Some(Some("W/m^2-K^4"))
        );
        assert_eq!(lookup_constant("pi#").unwrap().unit, None);
        assert!(lookup_constant("nope#").is_none());
    }

    // -- arithmetic operators -----------------------------------------------

    #[test]
    fn every_arithmetic_operator_evaluates() {
        let b = |op, l: f64, r: f64| ev(&Expr::bin(op, n(l), n(r)));
        assert_eq!(b(BinOp::Add, 2.0, 3.0), 5.0);
        assert_eq!(b(BinOp::Sub, 2.0, 3.0), -1.0);
        assert_eq!(b(BinOp::Mul, 2.0, 3.0), 6.0);
        assert_eq!(b(BinOp::Div, 3.0, 2.0), 1.5);
        assert_eq!(b(BinOp::Pow, 2.0, 10.0), 1024.0);
    }

    #[test]
    fn left_division_divides_by_the_left_operand() {
        // a \ b == b / a
        assert_eq!(ev(&Expr::bin(BinOp::LeftDiv, n(2.0), n(10.0))), 5.0);
        assert_eq!(ev(&Expr::bin(BinOp::LeftDiv, n(4.0), n(1.0))), 0.25);
    }

    #[test]
    fn element_wise_operators_use_their_scalar_equivalents() {
        assert_eq!(ev(&Expr::bin(BinOp::ElemMul, n(3.0), n(4.0))), 12.0);
        assert_eq!(ev(&Expr::bin(BinOp::ElemDiv, n(3.0), n(4.0))), 0.75);
        assert_eq!(ev(&Expr::bin(BinOp::ElemLeftDiv, n(4.0), n(3.0))), 0.75);
        assert_eq!(ev(&Expr::bin(BinOp::ElemPow, n(2.0), n(3.0))), 8.0);
    }

    #[test]
    fn negation_and_nesting() {
        // -(2 + 3) * 4
        let e = Expr::bin(
            BinOp::Mul,
            Expr::Neg(Box::new(Expr::bin(BinOp::Add, n(2.0), n(3.0)))),
            n(4.0),
        );
        assert_eq!(ev(&e), -20.0);
    }

    #[test]
    fn division_by_zero_is_an_error_not_an_infinity() {
        let message = err(&Expr::bin(BinOp::Div, n(1.0), n(0.0)));
        assert!(message.contains("division by zero"), "{message}");
        assert!(matches!(
            eval(&Expr::bin(BinOp::Div, n(1.0), n(0.0)), &Scope::new()),
            Err(FreesError::Evaluation { .. })
        ));
    }

    #[test]
    fn left_division_by_zero_is_an_error() {
        let message = err(&Expr::bin(BinOp::LeftDiv, n(0.0), n(1.0)));
        assert!(message.contains("division by zero"), "{message}");
        assert!(message.contains("left division"), "{message}");
    }

    #[test]
    fn element_wise_division_by_zero_is_also_an_error() {
        assert!(err(&Expr::bin(BinOp::ElemDiv, n(1.0), n(0.0))).contains("division by zero"));
        assert!(err(&Expr::bin(BinOp::ElemLeftDiv, n(0.0), n(1.0))).contains("division by zero"));
    }

    #[test]
    fn pow_domain_errors_replace_silent_nan_and_infinity() {
        let negative = err(&Expr::bin(BinOp::Pow, n(-8.0), n(0.5)));
        assert!(negative.contains("negative base"), "{negative}");
        let zero = err(&Expr::bin(BinOp::Pow, n(0.0), n(-1.0)));
        assert!(zero.contains("division by zero"), "{zero}");
        // Integral exponents on a negative base stay legal.
        assert_eq!(ev(&Expr::bin(BinOp::Pow, n(-2.0), n(3.0))), -8.0);
        assert_eq!(ev(&Expr::bin(BinOp::Pow, n(0.0), n(0.0))), 1.0);
    }

    #[test]
    fn errors_propagate_out_of_nested_subexpressions() {
        // 1 + (2 / 0)
        let e = Expr::bin(BinOp::Add, n(1.0), Expr::bin(BinOp::Div, n(2.0), n(0.0)));
        assert!(err(&e).contains("division by zero"));
    }

    // -- comparison, logic, truthiness ---------------------------------------

    #[test]
    fn every_comparison_operator_yields_exactly_one_or_zero() {
        let cmp = |op, l: f64, r: f64| {
            ev(&Expr::Compare {
                op,
                left: Box::new(n(l)),
                right: Box::new(n(r)),
            })
        };
        assert_eq!(cmp(CmpOp::Lt, 1.0, 2.0), 1.0);
        assert_eq!(cmp(CmpOp::Lt, 2.0, 1.0), 0.0);
        assert_eq!(cmp(CmpOp::Gt, 2.0, 1.0), 1.0);
        assert_eq!(cmp(CmpOp::Gt, 1.0, 2.0), 0.0);
        assert_eq!(cmp(CmpOp::Le, 2.0, 2.0), 1.0);
        assert_eq!(cmp(CmpOp::Le, 3.0, 2.0), 0.0);
        assert_eq!(cmp(CmpOp::Ge, 2.0, 2.0), 1.0);
        assert_eq!(cmp(CmpOp::Ge, 1.0, 2.0), 0.0);
        assert_eq!(cmp(CmpOp::Ne, 1.0, 2.0), 1.0);
        assert_eq!(cmp(CmpOp::Ne, 2.0, 2.0), 0.0);
        assert_eq!(cmp(CmpOp::Eq, 2.0, 2.0), 1.0);
        assert_eq!(cmp(CmpOp::Eq, 1.0, 2.0), 0.0);
    }

    #[test]
    fn logical_operators_use_the_java_truthiness_rule() {
        let logic = |op, l: f64, r: f64| {
            ev(&Expr::Logical {
                op,
                left: Box::new(n(l)),
                right: Box::new(n(r)),
            })
        };
        // Non-zero is true — including negatives and fractions.
        assert_eq!(logic(LogicOp::And, -1.0, 0.5), 1.0);
        assert_eq!(logic(LogicOp::And, 1.0, 0.0), 0.0);
        assert_eq!(logic(LogicOp::And, 0.0, 0.0), 0.0);
        assert_eq!(logic(LogicOp::Or, 0.0, -3.0), 1.0);
        assert_eq!(logic(LogicOp::Or, 0.0, 0.0), 0.0);
    }

    #[test]
    fn logical_operators_do_not_short_circuit_like_java() {
        // `0 and (1/0)` still evaluates the right operand, so the domain error
        // surfaces — matching the Java evaluator, which is eager on both sides.
        let e = Expr::Logical {
            op: LogicOp::And,
            left: Box::new(n(0.0)),
            right: Box::new(Expr::bin(BinOp::Div, n(1.0), n(0.0))),
        };
        assert!(err(&e).contains("division by zero"));
    }

    #[test]
    fn not_inverts_truthiness_to_one_or_zero() {
        let not = |v: f64| ev(&Expr::Not(Box::new(n(v))));
        assert_eq!(not(0.0), 1.0);
        assert_eq!(not(1.0), 0.0);
        assert_eq!(not(-2.5), 0.0);
        // Double negation normalises any truthy value to exactly 1.
        assert_eq!(ev(&Expr::Not(Box::new(Expr::Not(Box::new(n(42.0)))))), 1.0);
    }

    // -- non-scalar variants --------------------------------------------------

    #[test]
    fn array_access_range_and_literals_are_rejected_with_a_reason() {
        let access = err(&Expr::ArrayAccess {
            name: "speed".into(),
            indices: vec![n(1.0)],
        });
        assert!(access.contains("speed"), "{access}");
        assert!(access.contains("cannot be evaluated directly"), "{access}");

        let range = err(&Expr::Range {
            start: Box::new(n(1.0)),
            end: Box::new(n(5.0)),
        });
        assert!(range.contains("index range"), "{range}");

        let literal = err(&Expr::ArrayLiteral(vec![n(1.0), n(2.0)]));
        assert!(literal.contains("array literal"), "{literal}");
        assert!(literal.contains("2 element"), "{literal}");
    }

    // -- registry health -------------------------------------------------------

    #[test]
    fn the_registry_has_no_duplicate_or_mis_cased_names() {
        let mut seen = std::collections::BTreeSet::new();
        for intrinsic in INTRINSICS {
            assert_eq!(
                intrinsic.name,
                intrinsic.name.to_ascii_lowercase(),
                "intrinsic names must be lowercase: {}",
                intrinsic.name
            );
            assert!(
                seen.insert(intrinsic.name),
                "duplicate intrinsic: {}",
                intrinsic.name
            );
        }
        assert_eq!(seen.len(), INTRINSICS.len());
        assert_eq!(registry().len(), INTRINSICS.len());
    }

    #[test]
    fn every_registered_intrinsic_is_reachable_by_lookup() {
        for intrinsic in INTRINSICS {
            assert!(
                lookup_intrinsic(intrinsic.name).is_some(),
                "{} is not reachable",
                intrinsic.name
            );
        }
        assert!(lookup_intrinsic("definitely_not_a_function").is_none());
    }

    #[test]
    fn no_intrinsic_is_also_listed_as_unported() {
        for intrinsic in INTRINSICS {
            assert!(
                unported_family(intrinsic.name).is_none(),
                "{} is both implemented and marked unported",
                intrinsic.name
            );
        }
    }

    // -- elementary intrinsics: known values ------------------------------------

    #[test]
    fn abs_sign_and_the_power_shorthands() {
        assert_eq!(c("abs", &[-3.5]), 3.5);
        assert_eq!(c("abs", &[3.5]), 3.5);
        assert_eq!(c("sign", &[-2.0]), -1.0);
        assert_eq!(c("sign", &[2.0]), 1.0);
        assert_eq!(c("sign", &[0.0]), 0.0);
        assert_eq!(c("sqr", &[-4.0]), 16.0);
        assert_eq!(c("cube", &[-3.0]), -27.0);
        assert_eq!(c("hypot", &[3.0, 4.0]), 5.0);
    }

    #[test]
    fn roots_and_their_domains() {
        assert_eq!(c("sqrt", &[9.0]), 3.0);
        assert_eq!(c("sqrt", &[0.0]), 0.0);
        close(c("cbrt", &[-27.0]), -3.0);
        let message = cerr("sqrt", &[-1.0]);
        assert!(
            message.contains("square root of a negative number"),
            "{message}"
        );
    }

    #[test]
    fn exponential_and_logarithms() {
        close(c("exp", &[0.0]), 1.0);
        close(c("exp", &[1.0]), std::f64::consts::E);
        close(c("ln", &[std::f64::consts::E]), 1.0);
        close(c("log10", &[1000.0]), 3.0);
        close(c("log2", &[1024.0]), 10.0);
        // `log` is the base-10 alias.
        close(c("log", &[1000.0]), 3.0);
        // exp/ln round-trip
        close(c("ln", &[c("exp", &[2.75])]), 2.75);
    }

    #[test]
    fn logarithm_domain_errors() {
        for name in ["ln", "log10", "log2", "log"] {
            let zero = cerr(name, &[0.0]);
            assert!(zero.contains("logarithm of zero"), "{name}: {zero}");
            let negative = cerr(name, &[-1.0]);
            assert!(
                negative.contains("logarithm of a negative number"),
                "{name}: {negative}"
            );
        }
    }

    #[test]
    fn trigonometry_is_in_radians() {
        // sin(pi/6) = 0.5 only if the argument is radians; in degrees it would
        // be ~0.00914. This is the assertion that pins the angle convention.
        close(c("sin", &[std::f64::consts::PI / 6.0]), 0.5);
        close(c("cos", &[std::f64::consts::PI / 3.0]), 0.5);
        close(c("tan", &[std::f64::consts::PI / 4.0]), 1.0);
        assert!(
            (c("sin", &[30.0]) - 0.5).abs() > 1e-3,
            "30 must not be read as degrees"
        );
    }

    #[test]
    fn inverse_trigonometry_returns_radians_under_both_spellings() {
        for name in ["asin", "arcsin"] {
            close(c(name, &[0.5]), std::f64::consts::PI / 6.0);
        }
        for name in ["acos", "arccos"] {
            close(c(name, &[0.5]), std::f64::consts::PI / 3.0);
        }
        for name in ["atan", "arctan"] {
            close(c(name, &[1.0]), std::f64::consts::PI / 4.0);
        }
    }

    #[test]
    fn inverse_trigonometry_domain_errors() {
        for name in ["asin", "arcsin", "acos", "arccos"] {
            let message = cerr(name, &[1.5]);
            assert!(message.contains("must be in [-1, 1]"), "{name}: {message}");
            assert!(cerr(name, &[-1.5]).contains("must be in [-1, 1]"));
        }
        // The endpoints are legal.
        close(c("asin", &[1.0]), std::f64::consts::FRAC_PI_2);
        close(c("acos", &[-1.0]), std::f64::consts::PI);
    }

    #[test]
    fn atan2_is_four_quadrant_with_y_first() {
        close(c("atan2", &[1.0, 1.0]), std::f64::consts::PI / 4.0);
        close(c("atan2", &[1.0, -1.0]), 3.0 * std::f64::consts::PI / 4.0);
        close(c("atan2", &[-1.0, -1.0]), -3.0 * std::f64::consts::PI / 4.0);
        assert_eq!(c("atan2", &[0.0, 0.0]), 0.0);
    }

    #[test]
    fn hyperbolic_functions_and_their_inverses() {
        close(c("sinh", &[0.0]), 0.0);
        close(c("cosh", &[0.0]), 1.0);
        close(c("tanh", &[0.0]), 0.0);
        close(c("sinh", &[1.0]), 1.175_201_193_643_801_4);
        close(c("cosh", &[1.0]), 1.543_080_634_815_243_7);
        close(c("tanh", &[1.0]), 0.761_594_155_955_764_9);
        // Round-trips through the closed forms Java uses.
        close(c("arcsinh", &[c("sinh", &[0.7])]), 0.7);
        close(c("arccosh", &[c("cosh", &[0.7])]), 0.7);
        close(c("arctanh", &[c("tanh", &[0.7])]), 0.7);
    }

    #[test]
    fn inverse_hyperbolic_domain_errors() {
        let acosh = cerr("arccosh", &[0.5]);
        assert!(acosh.contains(">= 1.0"), "{acosh}");
        assert_eq!(c("arccosh", &[1.0]), 0.0);
        let atanh = cerr("arctanh", &[1.0]);
        assert!(atanh.contains("(-1, 1)"), "{atanh}");
        assert!(cerr("arctanh", &[-1.0]).contains("(-1, 1)"));
    }

    #[test]
    fn rounding_family_matches_java() {
        assert_eq!(c("floor", &[2.7]), 2.0);
        assert_eq!(c("floor", &[-2.1]), -3.0);
        assert_eq!(c("ceil", &[2.1]), 3.0);
        assert_eq!(c("ceil", &[-2.7]), -2.0);
        assert_eq!(c("trunc", &[2.9]), 2.0);
        assert_eq!(c("trunc", &[-2.9]), -2.0);
        assert_eq!(c("int", &[-2.9]), -2.0);
        assert_eq!(c("round", &[2.4]), 2.0);
        assert_eq!(c("round", &[2.5]), 3.0);
        // Java's Math.round breaks ties toward +infinity, so -2.5 -> -2.
        assert_eq!(c("round", &[-2.5]), -2.0);
        assert_eq!(c("round", &[-2.6]), -3.0);
    }

    #[test]
    fn round_takes_an_optional_digit_count() {
        close(c("round", &[1.23456, 2.0]), 1.23);
        close(c("round", &[1.23456, 4.0]), 1.2346);
        assert_eq!(c("round", &[1234.0, -2.0]), 1200.0);
        let message = cerr("round", &[1.0, 2.0, 3.0]);
        assert!(message.contains("1 to 2 arguments"), "{message}");
    }

    #[test]
    fn step_and_ramp_switch_at_the_origin() {
        assert_eq!(c("step", &[-0.001]), 0.0);
        assert_eq!(c("step", &[0.0]), 1.0);
        assert_eq!(c("step", &[3.0]), 1.0);
        assert_eq!(c("ramp", &[-3.0]), 0.0);
        assert_eq!(c("ramp", &[0.0]), 0.0);
        assert_eq!(c("ramp", &[3.0]), 3.0);
    }

    #[test]
    fn min_and_max_are_variadic() {
        assert_eq!(c("min", &[3.0]), 3.0);
        assert_eq!(c("min", &[3.0, -1.0, 7.0]), -1.0);
        assert_eq!(c("max", &[3.0, -1.0, 7.0]), 7.0);
        assert_eq!(c("max", &[-5.0]), -5.0);
        let message = cerr("min", &[]);
        assert!(message.contains("at least 1 argument"), "{message}");
    }

    #[test]
    fn mod_and_rem_take_the_sign_of_the_dividend() {
        assert_eq!(c("mod", &[7.0, 3.0]), 1.0);
        assert_eq!(c("mod", &[-7.0, 3.0]), -1.0);
        assert_eq!(c("mod", &[7.0, -3.0]), 1.0);
        assert_eq!(c("rem", &[-7.0, 3.0]), -1.0);
        close(c("mod", &[5.5, 2.0]), 1.5);
        let message = cerr("mod", &[1.0, 0.0]);
        assert!(message.contains("division by zero"), "{message}");
    }

    #[test]
    fn number_theory_and_bitwise_operations() {
        assert_eq!(c("gcd", &[12.0, 18.0]), 6.0);
        assert_eq!(c("gcd", &[-12.0, 18.0]), 6.0);
        assert_eq!(c("gcd", &[0.0, 0.0]), 0.0);
        assert_eq!(c("lcm", &[4.0, 6.0]), 12.0);
        assert_eq!(c("lcm", &[0.0, 5.0]), 0.0);
        assert_eq!(c("bitand", &[12.0, 10.0]), 8.0);
        assert_eq!(c("bitor", &[12.0, 10.0]), 14.0);
        assert_eq!(c("bitxor", &[12.0, 10.0]), 6.0);
        assert_eq!(c("bitnot", &[0.0]), -1.0);
        assert_eq!(c("bitshiftl", &[1.0, 10.0]), 1024.0);
        assert_eq!(c("bitshiftr", &[-1024.0, 3.0]), -128.0);
    }

    #[test]
    fn erf_gamma_and_factorial_known_values() {
        assert_eq!(c("erf", &[0.0]), 0.0);
        close(c("erf", &[1.0]), 0.842_700_792_949_714_9);
        close(c("erfc", &[1.0]), 1.0 - 0.842_700_792_949_714_9);
        close(c("gamma", &[5.0]), 24.0);
        close(c("gamma", &[0.5]), libm::sqrt(std::f64::consts::PI));
        close(c("factorial", &[0.0]), 1.0);
        close(c("factorial", &[5.0]), 120.0);
        close(c("factorial", &[10.0]), 3_628_800.0);
        close(c("loggamma", &[10.0]), libm::log(362_880.0));
        close(c("beta", &[2.0, 3.0]), 1.0 / 12.0);
    }

    #[test]
    fn gamma_and_factorial_domain_errors() {
        let pole = cerr("gamma", &[0.0]);
        assert!(pole.contains("pole at non-positive integer"), "{pole}");
        assert!(cerr("gamma", &[-3.0]).contains("pole"));
        // Non-integer negatives are fine.
        close(c("gamma", &[-0.5]), -2.0 * libm::sqrt(std::f64::consts::PI));
        let factorial = cerr("factorial", &[-1.5]);
        assert!(factorial.contains("must be > -1"), "{factorial}");
        assert!(cerr("factorial", &[-1.0]).contains("must be > -1"));
        assert!(cerr("beta", &[0.0, 1.0]).contains("must be > 0"));
    }

    /// `loggamma` is `log Γ(x)`, not `log |Γ(x)|`.
    ///
    /// The Java arm is Apache's `Gamma.logGamma`, which returns NaN for every
    /// `x <= 0` — Γ is negative on (−1, 0), (−3, −2), … so its logarithm does
    /// not exist there. `libm::lgamma` computes the *other* function
    /// `log |Γ(x)|` and returns a perfectly finite 1.2655… for `-0.5`, which is
    /// a value the reference engine never produces. Guard the whole
    /// non-positive half-line, not just the poles.
    #[test]
    fn loggamma_refuses_the_non_positive_half_line() {
        // Positive arguments are unaffected.
        close(c("loggamma", &[1.0]), 0.0);
        close(c("loggamma", &[10.0]), libm::log(362_880.0));
        close(
            c("loggamma", &[0.5]),
            libm::log(libm::sqrt(std::f64::consts::PI)),
        );

        for x in [-0.5, -1.5, -2.5, -0.0001] {
            let message = cerr("loggamma", &[x]);
            assert!(
                message.contains("must be > 0"),
                "loggamma({x}) must be refused, got {message}"
            );
            // `log |Γ|` really is finite there — that is the trap.
            assert!(libm::lgamma(x).is_finite(), "premise check for {x}");
        }
        assert!(cerr("loggamma", &[0.0]).contains("must be > 0"));
        assert!(cerr("loggamma", &[-3.0]).contains("must be > 0"));
        // `gamma` itself is unchanged: it is defined on the non-integer negatives.
        close(c("gamma", &[-0.5]), -2.0 * libm::sqrt(std::f64::consts::PI));
    }

    #[test]
    fn orthogonal_polynomials_match_their_closed_forms() {
        // P2(x) = (3x^2 - 1)/2
        close(c("legendrep", &[2.0, 0.3]), (3.0 * 0.09 - 1.0) / 2.0);
        assert_eq!(c("legendrep", &[0.0, 7.0]), 1.0);
        // T3(x) = 4x^3 - 3x
        close(c("chebyshevt", &[3.0, 0.4]), 4.0 * 0.064 - 1.2);
        // U2(x) = 4x^2 - 1
        close(c("chebyshevu", &[2.0, 0.4]), 4.0 * 0.16 - 1.0);
        // H3(x) = 8x^3 - 12x
        close(c("hermiteh", &[3.0, 0.5]), 8.0 * 0.125 - 6.0);
        // L2(x) = 1 - 2x + x^2/2
        close(c("laguerrel", &[2.0, 0.5]), 1.0 - 1.0 + 0.125);
        let message = cerr("legendrep", &[-1.0, 0.5]);
        assert!(message.contains("must be >= 0"), "{message}");
    }

    #[test]
    fn descriptive_statistics() {
        assert_eq!(c("mean", &[1.0, 2.0, 3.0, 4.0]), 2.5);
        assert_eq!(c("median", &[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(c("median", &[4.0, 1.0, 3.0, 2.0]), 2.5);
        // Unbiased (n-1) variance of 1..5 is 2.5.
        close(c("variance", &[1.0, 2.0, 3.0, 4.0, 5.0]), 2.5);
        close(c("var", &[1.0, 2.0, 3.0, 4.0, 5.0]), 2.5);
        assert_eq!(c("variance", &[7.0]), 0.0);
        close(c("stdev", &[1.0, 2.0, 3.0, 4.0, 5.0]), libm::sqrt(2.5));
        close(c("std", &[1.0, 2.0, 3.0, 4.0, 5.0]), libm::sqrt(2.5));
        close(c("stddev", &[1.0, 2.0, 3.0, 4.0, 5.0]), libm::sqrt(2.5));
        close(c("rms", &[3.0, 4.0]), libm::sqrt(12.5));
        close(c("average", &[1.0, 2.0, 6.0]), 3.0);
        close(c("avg", &[1.0, 2.0, 6.0]), 3.0);
        // Java's `average` returns 0 for an empty list; `mean` refuses.
        assert_eq!(c("average", &[]), 0.0);
        assert!(cerr("mean", &[]).contains("at least 1 argument"));
    }

    #[test]
    fn percentile_uses_the_apache_legacy_estimator() {
        // Apache's default estimator on sorted data: pos = p/100 * (n+1).
        assert_eq!(c("percentile", &[50.0, 1.0, 2.0, 3.0, 4.0, 5.0]), 3.0);
        assert_eq!(c("percentile", &[0.0, 5.0, 1.0, 3.0]), 1.0);
        assert_eq!(c("percentile", &[100.0, 5.0, 1.0, 3.0]), 5.0);
        close(c("percentile", &[25.0, 1.0, 2.0, 3.0, 4.0]), 1.25);
        let message = cerr("percentile", &[50.0]);
        assert!(message.contains("at least 2 argument"), "{message}");
    }

    #[test]
    fn normal_distribution_helpers() {
        close(c("normalcdf", &[0.0]), 0.5);
        close(c("normalcdf", &[1.0]), 0.841_344_746_068_543);
        close(c("normalcdf", &[10.0, 10.0, 2.0]), 0.5);
        close(
            c("normalpdf", &[0.0]),
            1.0 / libm::sqrt(2.0 * std::f64::consts::PI),
        );
        // P(-1 <= X <= 1) for the standard normal.
        close(
            c("probability", &[-1.0, 1.0, 0.0, 1.0]),
            0.682_689_492_137_086,
        );
        let message = cerr("normalcdf", &[0.0, 0.0, -1.0]);
        assert!(
            message.contains("standard deviation must be positive"),
            "{message}"
        );
        assert!(cerr("probability", &[0.0, 1.0, 0.0, 0.0]).contains("must be > 0"));
    }

    #[test]
    fn degenerate_complex_helpers_treat_a_real_as_z_with_zero_imaginary_part() {
        assert_eq!(c("real", &[2.5]), 2.5);
        assert_eq!(c("imag", &[2.5]), 0.0);
        assert_eq!(c("conj", &[2.5]), 2.5);
        assert_eq!(c("magnitude", &[-2.5]), 2.5);
        assert_eq!(c("angle", &[2.5]), 0.0);
        close(c("anglerad", &[-2.5]), std::f64::consts::PI);
        close(c("angledeg", &[-2.5]), 180.0);
        close(c("cis", &[0.0]), 1.0);
    }

    #[test]
    fn pi_is_available_as_both_a_constant_and_a_nullary_call() {
        assert_eq!(c("pi", &[]), std::f64::consts::PI);
        assert_eq!(ev(&Expr::var("pi#")), std::f64::consts::PI);
        let message = cerr("pi", &[1.0]);
        assert!(message.contains("0 arguments"), "{message}");
    }

    // -- if --------------------------------------------------------------------

    #[test]
    fn three_argument_if_selects_on_truthiness() {
        let e = |condition: f64| ev(&Expr::call("if", vec![n(condition), n(10.0), n(20.0)]));
        assert_eq!(e(1.0), 10.0);
        assert_eq!(e(-0.5), 10.0);
        assert_eq!(e(0.0), 20.0);
    }

    #[test]
    fn three_argument_if_only_evaluates_the_taken_branch() {
        // if(1, 5, 1/0) must not raise: the else branch is never touched.
        let taken = Expr::call(
            "if",
            vec![n(1.0), n(5.0), Expr::bin(BinOp::Div, n(1.0), n(0.0))],
        );
        assert_eq!(ev(&taken), 5.0);

        let not_taken = Expr::call(
            "if",
            vec![n(0.0), Expr::bin(BinOp::Div, n(1.0), n(0.0)), n(5.0)],
        );
        assert_eq!(ev(&not_taken), 5.0);
    }

    #[test]
    fn five_argument_if_is_the_frees_three_way_branch() {
        let e = |a: f64, b: f64| ev(&Expr::call("if", vec![n(a), n(b), n(-1.0), n(0.0), n(1.0)]));
        assert_eq!(e(1.0, 2.0), -1.0);
        assert_eq!(e(2.0, 2.0), 0.0);
        assert_eq!(e(3.0, 2.0), 1.0);
    }

    #[test]
    fn five_argument_if_evaluates_only_the_chosen_branch() {
        let boom = || Expr::bin(BinOp::Div, n(1.0), n(0.0));
        // a < b: only the `lt` branch runs.
        let e = Expr::call("if", vec![n(1.0), n(2.0), n(7.0), boom(), boom()]);
        assert_eq!(ev(&e), 7.0);
        // a > b: only the `gt` branch runs.
        let e = Expr::call("if", vec![n(3.0), n(2.0), boom(), boom(), n(9.0)]);
        assert_eq!(ev(&e), 9.0);
        // a == b: only the `eq` branch runs.
        let e = Expr::call("if", vec![n(2.0), n(2.0), boom(), n(8.0), boom()]);
        assert_eq!(ev(&e), 8.0);
    }

    /// **Recorded divergence from `Evaluator.evalIf`.**
    ///
    /// Java writes the three-way branch as `if (a < b) … else if (a == b) … else
    /// return eval(args.get(4))`, so when either operand is NaN both tests are
    /// false and it silently takes the *greater-than* branch. This port refuses
    /// instead, which is the honest answer but is not what the oracle does: a
    /// document that reaches `If` with a NaN gets a number from Java and a
    /// non-finite residual here. Pinned so the difference is visible rather than
    /// discovered during a parity run.
    #[test]
    fn five_argument_if_refuses_a_nan_comparison_where_java_falls_through() {
        // inf/inf is the reachable NaN: `/` only raises when the divisor is 0.
        let nan = Expr::bin(
            BinOp::Div,
            Expr::call("exp", vec![n(1000.0)]),
            Expr::call("exp", vec![n(1000.0)]),
        );
        assert!(ev(&nan).is_nan(), "premise: the operand really is NaN");

        let e = Expr::call("if", vec![nan, n(2.0), n(-1.0), n(0.0), n(1.0)]);
        let message = err(&e);
        assert!(
            message.contains("cannot compare"),
            "expected the NaN refusal, got {message}"
        );
        // Java would have returned the `gt` branch, 1.0, here.
    }

    #[test]
    fn if_rejects_arities_other_than_three_or_five() {
        for count in [0usize, 1, 2, 4, 6] {
            let args: Vec<Expr> = (0..count).map(|i| n(i as f64)).collect();
            let message = err(&Expr::call("if", args));
            assert!(message.contains("3 or 5 arguments"), "{count}: {message}");
        }
    }

    // -- sum / product ----------------------------------------------------------

    #[test]
    fn sum_and_product_over_a_plain_argument_list() {
        assert_eq!(c("sum", &[1.0, 2.0, 3.0]), 6.0);
        assert_eq!(c("sum", &[]), 0.0);
        assert_eq!(c("product", &[2.0, 3.0, 4.0]), 24.0);
        assert_eq!(c("product", &[]), 1.0);
    }

    #[test]
    fn sum_binds_its_index_variable_over_the_body() {
        // sum(i, 1, 4, i) = 10
        let e = Expr::call("sum", vec![Expr::var("i"), n(1.0), n(4.0), Expr::var("i")]);
        assert_eq!(ev(&e), 10.0);

        // sum(i, 1, 4, i^2) = 30
        let e = Expr::call(
            "sum",
            vec![
                Expr::var("i"),
                n(1.0),
                n(4.0),
                Expr::bin(BinOp::Pow, Expr::var("i"), n(2.0)),
            ],
        );
        assert_eq!(ev(&e), 30.0);
    }

    #[test]
    fn product_binds_its_index_variable_over_the_body() {
        // product(k, 1, 5, k) = 120
        let e = Expr::call(
            "product",
            vec![Expr::var("k"), n(1.0), n(5.0), Expr::var("k")],
        );
        assert_eq!(ev(&e), 120.0);
    }

    #[test]
    fn the_bound_index_shadows_and_then_restores_the_outer_scope() {
        let s = scope(&[("i", 100.0)]);
        // sum(i, 1, 3, i) sees 1, 2, 3 — not the outer 100.
        let inner = Expr::call("sum", vec![Expr::var("i"), n(1.0), n(3.0), Expr::var("i")]);
        assert_eq!(eval(&inner, &s).unwrap(), 6.0);
        // The outer binding is untouched afterwards.
        assert_eq!(eval(&Expr::var("i"), &s).unwrap(), 100.0);
        // And an `i` outside the body still resolves to the outer value.
        let mixed = Expr::bin(BinOp::Add, inner, Expr::var("i"));
        assert_eq!(eval(&mixed, &s).unwrap(), 106.0);
    }

    #[test]
    fn a_reduction_body_still_sees_the_enclosing_scope() {
        // sum(i, 1, 3, i * gain) with gain = 2 -> 12
        let s = scope(&[("gain", 2.0)]);
        let e = Expr::call(
            "sum",
            vec![
                Expr::var("i"),
                n(1.0),
                n(3.0),
                Expr::bin(BinOp::Mul, Expr::var("i"), Expr::var("gain")),
            ],
        );
        assert_eq!(eval(&e, &s).unwrap(), 12.0);
    }

    #[test]
    fn reduction_bounds_may_be_expressions_and_may_run_downward() {
        let s = scope(&[("nmax", 3.0)]);
        // sum(i, 1, nmax, i) = 6
        let e = Expr::call(
            "sum",
            vec![Expr::var("i"), n(1.0), Expr::var("nmax"), Expr::var("i")],
        );
        assert_eq!(eval(&e, &s).unwrap(), 6.0);

        // A descending range still runs (Java iterates with dir = -1).
        let down = Expr::call("sum", vec![Expr::var("i"), n(3.0), n(1.0), Expr::var("i")]);
        assert_eq!(ev(&down), 6.0);

        // lo == hi runs exactly once.
        let once = Expr::call(
            "product",
            vec![Expr::var("i"), n(4.0), n(4.0), Expr::var("i")],
        );
        assert_eq!(ev(&once), 4.0);
    }

    #[test]
    fn reduction_bounds_are_rounded_like_java() {
        // (int) Math.round(2.6) == 3
        let e = Expr::call("sum", vec![Expr::var("i"), n(1.0), n(2.6), Expr::var("i")]);
        assert_eq!(ev(&e), 6.0);
    }

    #[test]
    fn nested_reductions_bind_independently() {
        // sum(i, 1, 3, sum(j, 1, 2, i * j)) = sum_i 3*i = 18
        let inner = Expr::call(
            "sum",
            vec![
                Expr::var("j"),
                n(1.0),
                n(2.0),
                Expr::bin(BinOp::Mul, Expr::var("i"), Expr::var("j")),
            ],
        );
        let outer = Expr::call("sum", vec![Expr::var("i"), n(1.0), n(3.0), inner]);
        assert_eq!(ev(&outer), 18.0);
    }

    #[test]
    fn a_four_argument_reduction_without_a_variable_head_is_a_plain_list() {
        // sum(1, 2, 3, 4) — args[0] is not a Var, so it is not the bound form.
        assert_eq!(c("sum", &[1.0, 2.0, 3.0, 4.0]), 10.0);
        assert_eq!(c("product", &[1.0, 2.0, 3.0, 4.0]), 24.0);
    }

    #[test]
    fn a_runaway_reduction_is_refused_rather_than_hanging() {
        let e = Expr::call(
            "sum",
            vec![Expr::var("i"), n(1.0), n(1.0e9), Expr::var("i")],
        );
        let message = err(&e);
        assert!(message.contains("exceeds"), "{message}");
    }

    #[test]
    fn non_finite_reduction_bounds_are_refused() {
        let s = scope(&[("huge", f64::INFINITY)]);
        let e = Expr::call(
            "sum",
            vec![Expr::var("i"), n(1.0), Expr::var("huge"), Expr::var("i")],
        );
        let message = eval(&e, &s).unwrap_err().to_string();
        assert!(message.contains("not a finite number"), "{message}");

        // A finite but absurd bound is rejected too.
        let s = scope(&[("huge", 1.0e30)]);
        let e = Expr::call(
            "sum",
            vec![Expr::var("i"), n(1.0), Expr::var("huge"), Expr::var("i")],
        );
        let message = eval(&e, &s).unwrap_err().to_string();
        assert!(message.contains("out of range"), "{message}");
    }

    #[test]
    fn errors_inside_a_reduction_body_propagate() {
        // sum(i, -1, 1, ln(i)) fails at i = -1.
        let e = Expr::call(
            "sum",
            vec![
                Expr::var("i"),
                n(-1.0),
                n(1.0),
                Expr::call("ln", vec![Expr::var("i")]),
            ],
        );
        assert!(err(&e).contains("logarithm of a negative number"));
    }

    // -- string intrinsics -------------------------------------------------------

    #[test]
    fn string_intrinsics_read_literal_arguments() {
        let len = Expr::call("stringlen", vec![Expr::Str("hello".into())]);
        assert_eq!(ev(&len), 5.0);
        let val = Expr::call("stringval", vec![Expr::Str("  2.5 ".into())]);
        assert_eq!(ev(&val), 2.5);
        let pos = Expr::call(
            "stringpos",
            vec![Expr::Str("abcdef".into()), Expr::Str("cd".into())],
        );
        assert_eq!(ev(&pos), 3.0);
        let missing = Expr::call(
            "stringpos",
            vec![Expr::Str("abcdef".into()), Expr::Str("zz".into())],
        );
        assert_eq!(ev(&missing), 0.0);
    }

    #[test]
    fn string_intrinsics_reject_non_string_arguments() {
        let message = err(&Expr::call("stringlen", vec![n(1.0)]));
        assert!(message.contains("expected a string argument"), "{message}");
        let bad = err(&Expr::call("stringval", vec![Expr::Str("nope".into())]));
        assert!(bad.contains("is not a number"), "{bad}");
    }

    // -- error paths ---------------------------------------------------------------

    #[test]
    fn unknown_functions_say_so() {
        let message = err(&Expr::call("frobnicate", vec![n(1.0)]));
        assert_eq!(message, "evaluation error: unknown function: frobnicate");
    }

    #[test]
    fn unported_java_arms_are_refused_by_name_and_family() {
        for (name, family) in [
            ("integral", "calculus"),
            ("interpolate", "table lookup"),
            ("tablevalue", "parametric table"),
            ("odevalue", "ODE results"),
            ("arrayelmt", "arrays"),
            ("det", "matrices"),
            ("bode", "control systems"),
            ("erfinv", "special functions"),
            ("besselj", "Bessel functions"),
            ("random", "random numbers"),
            ("eos_z", "cubic EOS"),
        ] {
            let message = err(&Expr::call(name, vec![n(1.0)]));
            assert!(
                message.contains(&format!("not yet supported: {name}")),
                "{name}: {message}"
            );
            assert!(message.contains(family), "{name}: {message}");
        }
    }

    #[test]
    fn synthetic_dollar_calls_are_refused_as_a_family() {
        for name in [
            "prop$enthalpy$water$t$p",
            "proc$mypro$0",
            "eigen$val$1$3",
            "det$3",
            "series$0$2$2",
        ] {
            let message = err(&Expr::call(name, vec![n(1.0)]));
            assert!(message.contains("not yet supported"), "{name}: {message}");
            assert!(message.contains("synthetic"), "{name}: {message}");
        }
    }

    #[test]
    fn correlation_prefixes_are_refused_as_a_family() {
        for name in [
            "htc_1phase",
            "dp_2phase",
            "nu_gnielinski",
            "void_zivi",
            "mix_cp",
        ] {
            let message = err(&Expr::call(name, vec![n(1.0)]));
            assert!(message.contains("not yet supported"), "{name}: {message}");
            assert!(
                message.contains("engineering correlation library"),
                "{name}: {message}"
            );
        }
    }

    #[test]
    fn wrong_arity_is_reported_with_the_expected_shape() {
        let too_many = err(&Expr::call("sqrt", vec![n(1.0), n(2.0)]));
        assert!(
            too_many.contains("sqrt expects 1 argument, got 2"),
            "{too_many}"
        );
        let too_few = err(&Expr::call("atan2", vec![n(1.0)]));
        assert!(
            too_few.contains("atan2 expects 2 arguments, got 1"),
            "{too_few}"
        );
        let none = err(&Expr::call("abs", vec![]));
        assert!(none.contains("abs expects 1 argument, got 0"), "{none}");
    }

    #[test]
    fn every_error_is_an_evaluation_error_never_a_panic() {
        let cases = vec![
            Expr::var("missing"),
            Expr::Str("s".into()),
            Expr::bin(BinOp::Div, n(1.0), n(0.0)),
            Expr::call("sqrt", vec![n(-1.0)]),
            Expr::call("nope", vec![]),
            Expr::call("integral", vec![]),
            Expr::ArrayLiteral(vec![]),
            Expr::Range {
                start: Box::new(n(1.0)),
                end: Box::new(n(2.0)),
            },
            Expr::ArrayAccess {
                name: "a".into(),
                indices: vec![n(1.0)],
            },
        ];
        for case in cases {
            match eval(&case, &Scope::new()) {
                Err(FreesError::Evaluation { .. }) => {}
                other => panic!("expected an Evaluation error for {case:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn no_implemented_intrinsic_returns_a_silent_nan_on_a_legal_input() {
        // A representative sweep: every unary intrinsic on a benign argument.
        for name in [
            "abs",
            "sign",
            "sqrt",
            "cbrt",
            "sqr",
            "cube",
            "exp",
            "ln",
            "log10",
            "log2",
            "log",
            "floor",
            "ceil",
            "trunc",
            "int",
            "round",
            "sin",
            "cos",
            "tan",
            "asin",
            "acos",
            "atan",
            "arcsin",
            "arccos",
            "arctan",
            "sinh",
            "cosh",
            "tanh",
            "arcsinh",
            "arctanh",
            "step",
            "ramp",
            "erf",
            "erfc",
            "gamma",
            "loggamma",
            "factorial",
            "real",
            "imag",
            "conj",
            "magnitude",
            "angle",
            "anglerad",
            "angledeg",
            "cis",
            "mean",
            "median",
            "rms",
            "normalcdf",
            "normalpdf",
        ] {
            let v = c(name, &[0.5]);
            assert!(!v.is_nan(), "{name}(0.5) returned NaN");
        }
    }

    // -- composed / integration-ish -------------------------------------------------

    #[test]
    fn a_realistic_expression_tree_evaluates_end_to_end() {
        // eta = 1 - (T_c / T_h)
        let s = scope(&[("t_c", 300.0), ("t_h", 500.0)]);
        let eta = Expr::bin(
            BinOp::Sub,
            n(1.0),
            Expr::bin(BinOp::Div, Expr::var("T_c"), Expr::var("T_h")),
        );
        close(eval(&eta, &s).unwrap(), 0.4);

        // max(0, round(eta * 100, 1))
        let rounded = Expr::call(
            "max",
            vec![
                n(0.0),
                Expr::call("round", vec![Expr::bin(BinOp::Mul, eta, n(100.0)), n(1.0)]),
            ],
        );
        close(eval(&rounded, &s).unwrap(), 40.0);
    }

    #[test]
    fn a_conditional_over_a_bound_sum_composes() {
        // if(sum(i, 1, n, i) > 5, 1, 0) with n = 4 -> sum is 10 -> 1
        let s = scope(&[("n", 4.0)]);
        let total = Expr::call(
            "sum",
            vec![Expr::var("i"), n(1.0), Expr::var("n"), Expr::var("i")],
        );
        let e = Expr::call(
            "if",
            vec![
                Expr::Compare {
                    op: CmpOp::Gt,
                    left: Box::new(total),
                    right: Box::new(n(5.0)),
                },
                n(1.0),
                n(0.0),
            ],
        );
        assert_eq!(eval(&e, &s).unwrap(), 1.0);
    }

    #[test]
    fn env_lookup_walks_the_binding_chain() {
        let s = scope(&[("a", 1.0), ("b", 2.0)]);
        let root = Env::Root(&s);
        let inner = Env::Bind {
            name: "b",
            value: 99.0,
            parent: &root,
        };
        let innermost = Env::Bind {
            name: "cc",
            value: 7.0,
            parent: &inner,
        };
        assert_eq!(innermost.get("a"), Some(1.0));
        assert_eq!(innermost.get("b"), Some(99.0));
        assert_eq!(innermost.get("cc"), Some(7.0));
        assert_eq!(innermost.get("d"), None);
        assert_eq!(root.get("b"), Some(2.0));
    }

    #[test]
    fn eval_in_is_usable_directly_with_a_custom_environment() {
        let s = Scope::new();
        let root = Env::Root(&s);
        let bound = Env::Bind {
            name: "x",
            value: 4.0,
            parent: &root,
        };
        let e = Expr::call("sqrt", vec![Expr::var("x")]);
        assert_eq!(eval_in(&e, &bound).unwrap(), 2.0);
    }

    #[test]
    fn java_helpers_match_their_java_semantics() {
        assert_eq!(java_round(0.5), 1.0);
        assert_eq!(java_round(-0.5), 0.0);
        assert_eq!(java_round(1.5), 2.0);
        assert_eq!(java_round(-1.5), -1.0);
        assert!(java_round(f64::NAN).is_nan());

        assert_eq!(java_signum(-0.0), 0.0);
        assert!(java_signum(-0.0).is_sign_negative());
        assert!(java_signum(f64::NAN).is_nan());

        assert!(java_min(1.0, f64::NAN).is_nan());
        assert!(java_max(1.0, f64::NAN).is_nan());
        assert_eq!(java_min(2.0, 3.0), 2.0);
        assert_eq!(java_max(2.0, 3.0), 3.0);
        assert!(java_min(-0.0, 0.0).is_sign_negative());
        assert!(java_max(-0.0, 0.0).is_sign_positive());

        assert_eq!(gcd_i64(0, 5), 5);
        assert_eq!(gcd_i64(-12, -18), 6);
        assert!((mean(&[1.0, 2.0]) - 1.5).abs() < EPS);
    }
}
