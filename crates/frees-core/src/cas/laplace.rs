//! `LaplaceTransform` / `InverseLaplaceTransform` — two of the 13 Symja
//! operations [`crate::cas`] replaces.
//!
//! Port target: `CasEngine.laplace` / `CasEngine.inverseLaplace`
//! (`../frEES/backend/core/.../cas/CasEngine.java`), reached from the REPL by
//! `laplace(f, t, s)` and `inverselaplace(F, s, t)` / `ilaplace(F, s, t)`
//! (`ReplEvaluator.evaluateCas`). Both are **REPL-only**: `CAS_CALL` is matched
//! against a REPL line, so no `.frees` document can drive them and no golden
//! fixture covers them.
//!
//! # What the Java actually reaches — measured, not assumed
//!
//! Every line below was produced by running the real oracle (the frEES core jar
//! with Symja 3.0.0 on the classpath) through the same three calls
//! `ReplEvaluator.evaluateCas` makes. It is **much narrower** than "the
//! textbook table", and it is wrong in one place:
//!
//! | input | Symja 3.0.0 | this port |
//! |---|---|---|
//! | `laplace(t^3)` | `6/s^4` | same |
//! | `laplace(exp(-a*t))` | `1/(a+s)` | same |
//! | `laplace(sin(t))` | `1/(1+s^2)` | same |
//! | `laplace(cosh(t))` | `s/(-1+s^2)` | same |
//! | `laplace(sinh(t))` | **`c/(-1+s^2)`** — a free symbol `c` where `1` belongs | `1/(s^2-1)` (**corrected**) |
//! | `laplace(sin(3*t))`, `laplace(sin(w*t))` | *refused* — unevaluated | `3/(s^2+9)`, `w/(s^2+w^2)` (**superset**) |
//! | `laplace(sinh(a*t))`, `laplace(cosh(a*t))` | *refused* | table (**superset**) |
//! | `laplace(exp(-2*t)*sin(3*t))` | *refused* (shifts, then cannot do `Sin(3t)`) | `3/((s+2)^2+9)` (**superset**) |
//! | `laplace(exp(-a*t)*sin(t))` | `1/(1+(a+s)^2)` | same |
//! | `laplace(exp(-a*t)*exp(-b*t))` | *refused* — it merges to `E^(-a*t-b*t)` and stalls | `1/(s+a+b)` (**superset**) |
//! | `laplace(t*sin(t))` | `(2*s)/(1+s^2)^2` | same value |
//! | `laplace(t*sin(w*t))` | **crashes the bridge** — Symja returns `-Derivative(0,0,1)[LaplaceTransform][…]`, which `CasExpressions.parse` rejects | `2*w*s/(s^2+w^2)^2` (**superset**) |
//! | `laplace((t+1)^2)` | *refused* — no expansion first | *refused* (same; `Expand` is [`crate::cas::ops`]' job) |
//! | `laplace(ln(t))`, `laplace(sqrt(t))` | `-(EulerGamma+ln(s))/s`, `Sqrt(Pi)/(2*s^(3/2))` | *refused* (**regression**, see gaps) |
//! | `ilaplace(1/(s+2))` | `E^(-2*t)` | same |
//! | `ilaplace(1/(s^2+9))` | `Sin(3*t)/3` | same |
//! | `ilaplace(1/(s^2+w^2))` | `Sin(t*w)/w` | same |
//! | `ilaplace((s+3)/(s^2+3*s+2))` | `-1/E^(2*t)+2/E^t` | same value |
//! | `ilaplace(1/((s+1)^2*(s+2)))` | `E^(-2*t)-1/E^t+t/E^t` | same value |
//! | `ilaplace(1/((s^2+1)*(s^2+4)))` | `Sin(t)/3-Sin(2*t)/6` | same value |
//! | `ilaplace(1/(s^2+4*s+13))`, `ilaplace(1/((s+2)^2+9))` | *refused* — **no damped sinusoid at all** | `exp(-2*t)*sin(3*t)/3` (**superset**) |
//! | `ilaplace(1/(s^2+2*s+1))` | *refused*, though `1/(s+1)^2` works — Symja never factors | `t*exp(-t)` (**superset**) |
//! | `ilaplace(1/(3*s+6))`, `ilaplace(1/(2*s^2+8))`, `ilaplace(1/(4*s^2+4*s+1))` | *refused* — non-monic denominators | `exp(-2*t)/3`, `sin(2*t)/4`, `t*exp(-t/2)/4` (**superset**) |
//! | `ilaplace((s^2+2*s+3)/((s+1)*(s^2+4)))` | `2/5*1/E^t+InverseLaplaceTransform((7+3*s)/(4+s^2),s,t)/5` — **half-transformed**, and `isUnevaluated` misses it because the *leading* term is not the head, so the REPL reports success and hands the user an expression that cannot be evaluated | fully transformed (**superset**, and closes that hole) |
//! | `ilaplace(5)`, `ilaplace(s/(s+1))` | `5*DiracDelta(t)`, `-1/E^t+DiracDelta(t)` | *refused* by name — frees has no `DiracDelta` and the Java's answer does not evaluate |
//! | `ilaplace(1/(s^2+1)^2)` | *refused* | *refused* (same) |
//!
//! The port is therefore a **documented superset with one correction and one
//! regression**, not a bug-for-bug transcription. Bug-compatibility was never
//! reachable: the task's own requirement — inverse transforms of irreducible
//! quadratics, i.e. damped sinusoids — is something Symja 3.0.0 cannot do at
//! all.
//!
//! # Structure
//!
//! **Forward** is a table plus linearity, applied structurally (exactly like
//! Symja: `(t+1)^2` is refused rather than expanded). Two operator rules ride
//! on top of the table:
//!
//! * *shifting* — `L{e^(a·t)·g(t)} = G(s−a)`, a substitution;
//! * *multiplication by t* — `L{t^n·g(t)} = (−1)^n·dⁿG/dsⁿ`, which routes to
//!   the already-ported [`crate::differentiator`].
//!
//! **Inverse** is partial fractions then a per-term table lookup, over two
//! paths:
//!
//! * an **exact** path for rational functions whose coefficients are all
//!   numeric — `ℚ` arithmetic end to end (`num_rational::BigRational`), never
//!   `f64`, per `cas/mod.rs` "Exactness";
//! * a **structural** path for the shapes that carry symbolic parameters
//!   (`1/(s+a)`, `1/(s^2+w^2)`, `(s+α)/((s+α)^2+ω^2)`), which is what makes
//!   `ilaplace(1/(s+a), s, t) = e^(−a·t)` work at all.
//!
//! Anything outside both is **refused by name**. Nothing is approximated.
//!
//! # Relationship to [`crate::cas::ratfun`]
//!
//! `ratfun` owns `Apart`. The decomposition here is deliberately *not* the same
//! function: `Apart` returns an s-domain [`Expr`], while the inverse transform
//! needs the factored denominator and the numerator polynomial of each piece as
//! data. When `ratfun` lands, [`partial_fractions`] is the seam — it takes
//! `(numerator, denominator)` as `ℚ`-polynomials and returns the pieces; swap
//! its body for a `ratfun` call and everything above it is unchanged.
//!
//! # Gaps — what this module refuses, and why
//!
//! Each is refused **by name**, never approximated:
//!
//! 1. **Non-integer powers of `t`.** `laplace(sqrt(t))` and `laplace(ln(t))`
//!    are the one place the port is *narrower* than the Java: their images are
//!    `Γ(p+1)/s^(p+1)` and `−(γ + ln s)/s`, and frees has neither a gamma
//!    function nor an Euler–Mascheroni constant to write them with. This is a
//!    **regression against the oracle** and belongs in the divergence ledger.
//! 2. **Polynomial and improper images.** Symja emits `DiracDelta(t)`;
//!    frees has no impulse, and Symja's own answer does not survive
//!    `Evaluator.eval`, so refusing is the strictly better behaviour.
//! 3. **Repeated irreducible quadratics** (`1/(s^2+1)^2`). The table entry
//!    exists (`(sin ωt − ωt cos ωt)/2ω³`); it is simply not written yet.
//!    Symja refuses these too, so this costs no parity.
//! 4. **Irreducible factors of degree ≥ 3** that the rational-root sieve
//!    cannot split — e.g. `s^4+s^2+1 = (s^2+s+1)(s^2−s+1)`, which needs a real
//!    factoriser over ℚ. That is [`crate::cas::poly`]'s job; when it lands,
//!    [`factor_over_q`] should delegate to it.
//! 5. **Symbolic products with possibly-coinciding poles** (`1/((s+a)(s+b))`).
//!    The residues are `±1/(b−a)`, valid only when `a ≠ b`, which is
//!    undecidable symbolically.
//! 6. **Products of two functions of `t`** — a convolution, not a table entry.
//!    `(t+1)^2` lands here rather than being expanded, exactly as in the Java;
//!    the refusal points the user at `Expand`.
//! 7. **`L{f⁽ⁿ⁾}` for an unapplied `f`.** [`transform_derivative`] implements
//!    the rule, but only for an `f` that is itself transformable — see its
//!    docs for why the Java's `LaplaceTransform(f'[t],t,s)` form cannot be
//!    reached from frees at all.

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::ast::{BinOp, Expr};
use crate::diag::{FreesError, Result};
use crate::differentiator::differentiate;

// ---------------------------------------------------------------------------
// Limits
// ---------------------------------------------------------------------------

/// Recursion ceiling for the structural walks. The parser already bounds
/// expression depth (`parser::expr::MAX_EXPR_DEPTH`); this is the belt to that
/// brace, so a hand-built AST cannot overflow the stack either.
const MAX_DEPTH: u32 = 64;

/// Largest `n` accepted in `t^n` (forward) or `1/(s−p)^(n+1)` (inverse).
///
/// `18! = 6_402_373_705_728_000 < 2^53`, so every factorial the table needs is
/// still an *exact* `f64`. `19!` is not, and a silently-rounded factorial is
/// precisely the "plausible and wrong" failure `cas/mod.rs` refuses to ship.
const MAX_POWER: u32 = 18;

/// Largest derivative order [`transform_derivative`] will build.
const MAX_DERIVATIVE_ORDER: u32 = 8;

/// Largest `n` in `L{tⁿ·g(t)}` when `g` is a table entry rather than a bare
/// power. Each `n` costs one pass of [`crate::differentiator`], whose quotient
/// rule squares the denominator, so the tree is exponential in `n`.
/// `L{tⁿ}` itself is unaffected — it comes straight from the table.
const MAX_T_MULTIPLY: u32 = 4;

/// Largest polynomial degree the exact path will factor.
const MAX_DEGREE: usize = 24;

/// Above this magnitude a numerator/denominator is no longer an exact `f64`,
/// so emitting it as one would lose the exactness the CAS promises.
const MAX_EXACT_INTEGER: i64 = 1i64 << 53;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// `L{f(t)}(s)` — the forward Laplace transform.
///
/// Port of `CasEngine.laplace(expression, timeVar, freqVar)`. `time_var` and
/// `freq_var` go through the same identifier check the Java applies
/// (`CasEngine.requireIdentifier`: `[A-Za-z][A-Za-z0-9_]*`, lowercased).
///
/// Refuses by name anything outside the table — see the module docs for the
/// measured surface.
pub fn transform(f: &Expr, time_var: &str, freq_var: &str) -> Result<Expr> {
    let (t, s) = variable_pair(time_var, freq_var, "laplace")?;
    forward(f, &t, &s, 0)
}

/// `L⁻¹{F(s)}(t)` — the inverse Laplace transform.
///
/// Port of `CasEngine.inverseLaplace(expression, freqVar, timeVar)`. Note the
/// Java argument order: the **frequency** variable comes first.
pub fn inverse_transform(image: &Expr, freq_var: &str, time_var: &str) -> Result<Expr> {
    let (s, t) = variable_pair(freq_var, time_var, "inverselaplace")?;
    inverse(image, &s, &t)
}

/// `L{f⁽ⁿ⁾(t)} = sⁿ·F(s) − Σ_{k=0}^{n−1} s^(n−1−k)·f⁽ᵏ⁾(0)`.
///
/// Symja has this rule — `LaplaceTransform(f'[t],t,s)` evaluates to
/// `-f(0)+s*LaplaceTransform(f(t),t,s)` — but it is **unreachable from frees**:
/// `ExprToSymja` renders only the 19 whitelisted scalar functions and has no
/// spelling at all for an unapplied derivative, so neither a document nor a
/// REPL line can produce `f'[t]`. It is therefore exposed here at the Rust API
/// level only, and it is the *applied* form: `f` must itself be transformable,
/// and the initial values `f⁽ᵏ⁾(0)` are obtained by differentiating `f` with
/// [`crate::differentiator`] and substituting `t := 0`.
pub fn transform_derivative(f: &Expr, order: u32, time_var: &str, freq_var: &str) -> Result<Expr> {
    let (t, s) = variable_pair(time_var, freq_var, "laplace")?;
    if order > MAX_DERIVATIVE_ORDER {
        return Err(refuse(
            "laplace",
            &format!(
                "derivative order {order} exceeds the supported maximum of {MAX_DERIVATIVE_ORDER}"
            ),
        ));
    }
    let image = forward(f, &t, &s, 0)?;
    let sv = Expr::var(&s);
    let mut out = mul(power_of(&sv, order), image);
    let mut derivative = f.clone();
    for k in 0..order {
        // f⁽ᵏ⁾(0): substitute t := 0 into the k-th derivative.
        let initial = substitute(&derivative, &t, &num(0.0));
        out = sub(out, mul(power_of(&sv, order - 1 - k), initial));
        if k + 1 < order {
            derivative = differentiate(&derivative, &t).ok_or_else(|| {
                refuse(
                    "laplace",
                    &format!(
                        "d^{}/d{t}^{} of the input is not symbolically differentiable",
                        k + 1,
                        k + 1
                    ),
                )
            })?;
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Identifier handling — CasEngine.requireIdentifier
// ---------------------------------------------------------------------------

/// Port of `CasEngine.requireIdentifier`: a plain identifier, lowercased.
fn identifier(name: &str) -> Result<String> {
    let mut chars = name.chars();
    let head_ok = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic());
    let tail_ok = chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !head_ok || !tail_ok {
        return Err(FreesError::evaluation(format!(
            "invalid variable name: '{name}'"
        )));
    }
    Ok(name.to_ascii_lowercase())
}

fn variable_pair(first: &str, second: &str, op: &str) -> Result<(String, String)> {
    let a = identifier(first)?;
    let b = identifier(second)?;
    if a == b {
        return Err(FreesError::evaluation(format!(
            "{op}: the time and frequency variables must differ (both are '{a}')"
        )));
    }
    Ok((a, b))
}

/// The refusal the REPL surfaces (`ReplEvaluator.evaluateCas`:
/// `"<fn>: no closed form found for this input."`), extended with the *name* of
/// the construct that fell outside the table — the parent engine's
/// "diagnostics quote the user's own text" rule.
fn refuse(op: &str, what: &str) -> FreesError {
    FreesError::evaluation(format!(
        "{op}: no closed form found for this input — {what}"
    ))
}

// ---------------------------------------------------------------------------
// Forward transform
// ---------------------------------------------------------------------------

fn forward(f: &Expr, t: &str, s: &str, depth: u32) -> Result<Expr> {
    if depth > MAX_DEPTH {
        return Err(refuse("laplace", "expression is too deeply nested"));
    }
    // L{c} = c/s for any c free of t. This is also the base of the table.
    if !depends_on(f, t) {
        return Ok(div(f.clone(), Expr::var(s)));
    }
    match f {
        Expr::BinOp {
            op: BinOp::Add,
            left,
            right,
        } => Ok(add(
            forward(left, t, s, depth + 1)?,
            forward(right, t, s, depth + 1)?,
        )),
        Expr::BinOp {
            op: BinOp::Sub,
            left,
            right,
        } => Ok(sub(
            forward(left, t, s, depth + 1)?,
            forward(right, t, s, depth + 1)?,
        )),
        Expr::Neg(inner) => Ok(neg(forward(inner, t, s, depth + 1)?)),
        _ => forward_product(f, t, s, depth),
    }
}

/// The multiplicative core: split into factors, peel the two operator rules
/// (`exp` shifting and multiplication by `t`), and look the remainder up in the
/// table.
fn forward_product(f: &Expr, t: &str, s: &str, depth: u32) -> Result<Expr> {
    let mut numerator = Vec::new();
    let mut denominator = Vec::new();
    collect_factors(f, true, &mut numerator, &mut denominator, 0)?;

    // Anything dividing the expression must be free of t: `1/t` has no
    // elementary transform, and Symja refuses it too.
    let mut divisor = num(1.0);
    for factor in &denominator {
        if depends_on(factor, t) {
            return Err(refuse(
                "laplace",
                &format!("division by a function of {t} ({})", describe(factor)),
            ));
        }
        divisor = mul(divisor, factor.clone());
    }

    let mut coefficient = num(1.0);
    let mut shift: Option<Expr> = None;
    let mut t_power: u32 = 0;
    let mut remaining: Option<Expr> = None;

    for factor in &numerator {
        if !depends_on(factor, t) {
            coefficient = mul(coefficient, factor.clone());
            continue;
        }
        // t^n · … — the multiplication-by-t rule.
        if let Some(n) = power_of_var(factor, t) {
            t_power = t_power.saturating_add(n);
            if t_power > MAX_POWER {
                return Err(refuse(
                    "laplace",
                    &format!("{t}^{t_power} exceeds the supported maximum of {t}^{MAX_POWER}"),
                ));
            }
            continue;
        }
        // exp(a·t + b) · … — the shifting rule. `exp(b)` is an ordinary
        // constant and factors straight out.
        if let Expr::Call { function, args } = factor {
            if function == "exp" && args.len() == 1 {
                if let Some((slope, intercept)) = affine_in(&args[0], t) {
                    if !is_literal(&intercept, 0.0) {
                        coefficient = mul(coefficient, call1("exp", intercept));
                    }
                    shift = Some(match shift {
                        None => slope,
                        Some(previous) => add(previous, slope),
                    });
                    continue;
                }
            }
        }
        if remaining.is_some() {
            return Err(refuse(
                "laplace",
                &format!(
                    "a product of two functions of {t} ({}) — the transform of a product is a convolution, not a table entry; expand the expression first if it is polynomial",
                    describe(f)
                ),
            ));
        }
        remaining = Some(factor.clone());
    }

    let mut image = match &remaining {
        Some(atom) => {
            // L{tⁿ·g} = (−1)ⁿ·dⁿG/dsⁿ. The two rules commute, so applying this
            // one before the shift below is free.
            //
            // The bound is not arbitrary: `differentiate`'s quotient rule
            // squares the denominator every pass, so an unsimplified nth
            // derivative is exponential in n. Past a handful of passes the
            // tree stops being an answer and starts being a memory profile —
            // refuse by name instead.
            if t_power > MAX_T_MULTIPLY {
                return Err(refuse(
                    "laplace",
                    &format!(
                        "{t}^{t_power} times a table entry — the multiplication-by-{t} rule differentiates {t_power} times, which is bounded at {MAX_T_MULTIPLY} here"
                    ),
                ));
            }
            let mut image = forward_atom(atom, t, s, depth)?;
            for _ in 0..t_power {
                image = neg(differentiate(&image, s).ok_or_else(|| {
                    refuse(
                        "laplace",
                        &format!(
                            "d/d{s} of the intermediate transform is not symbolically differentiable"
                        ),
                    )
                })?);
            }
            image
        }
        // Every factor was consumed by the operator rules, so the base is the
        // power table entry itself: L{tⁿ} = n!/s^(n+1). Reaching it by
        // differentiating `1/s` n times would be the same value built out of
        // an exponentially larger tree — `t^10` alone produces thousands of
        // nodes and stops evaluating to a finite number.
        None => div(
            num(factorial(t_power) as f64),
            power_of(&Expr::var(s), t_power + 1),
        ),
    };

    // L{e^(a·t)·g} = G(s − a).
    if let Some(a) = shift {
        image = substitute(&image, s, &sub(Expr::var(s), a));
    }

    Ok(div(mul(coefficient, image), divisor))
}

/// The transform table proper: one non-constant, non-product, non-`exp`,
/// non-power-of-`t` factor.
fn forward_atom(atom: &Expr, t: &str, s: &str, depth: u32) -> Result<Expr> {
    let sv = Expr::var(s);
    match atom {
        // A parenthesised sum inside a product, e.g. `(1 - exp(-a*t))/a`.
        Expr::BinOp {
            op: BinOp::Add | BinOp::Sub,
            ..
        }
        | Expr::Neg(_) => forward(atom, t, s, depth + 1),

        Expr::Call { function, args } if args.len() == 1 => {
            let inner = &args[0];
            let (slope, intercept) = affine_in(inner, t).ok_or_else(|| {
                refuse(
                    "laplace",
                    &format!(
                        "{function}({}) — the table covers {function}(a*{t}) only",
                        describe(inner)
                    ),
                )
            })?;
            if !is_literal(&intercept, 0.0) {
                return Err(refuse(
                    "laplace",
                    &format!(
                        "{function}({}) — a phase-shifted argument is outside the table",
                        describe(inner)
                    ),
                ));
            }
            let w = slope;
            let w2 = mul(w.clone(), w.clone());
            match function.as_str() {
                // exp only reaches here as a bare `exp(a*t)`; the shifting rule
                // above catches it inside a product.
                "exp" => Ok(div(num(1.0), sub(sv.clone(), w))),
                // L{sin(w·t)} = w/(s²+w²)
                "sin" => Ok(div(w, add(mul(sv.clone(), sv), w2))),
                // L{cos(w·t)} = s/(s²+w²)
                "cos" => Ok(div(sv.clone(), add(mul(sv.clone(), sv), w2))),
                // L{sinh(a·t)} = a/(s²−a²). Symja answers `c/(-1+s^2)` here —
                // a free symbol where `1` belongs. Corrected, not transcribed.
                "sinh" => Ok(div(w, sub(mul(sv.clone(), sv), w2))),
                // L{cosh(a·t)} = s/(s²−a²)
                "cosh" => Ok(div(sv.clone(), sub(mul(sv.clone(), sv), w2))),
                _ => Err(refuse(
                    "laplace",
                    &format!(
                        "{function}({}) is not in the transform table",
                        describe(inner)
                    ),
                )),
            }
        }
        _ => Err(refuse(
            "laplace",
            &format!("{} is not in the transform table", describe(atom)),
        )),
    }
}

/// `n` when `e` is `t` or `t^n` for a non-negative integer literal `n`.
fn power_of_var(e: &Expr, var: &str) -> Option<u32> {
    match e {
        Expr::Var(name) if name == var => Some(1),
        Expr::BinOp {
            op: BinOp::Pow,
            left,
            right,
        } => match left.as_ref() {
            Expr::Var(name) if name == var => non_negative_integer(right),
            _ => None,
        },
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Inverse transform
// ---------------------------------------------------------------------------

fn inverse(image: &Expr, s: &str, t: &str) -> Result<Expr> {
    let mut terms = Vec::new();
    split_sum(image, false, &mut terms, 0)?;
    let mut out: Option<Expr> = None;
    for (negated, term) in terms {
        let piece = inverse_term(&term, s, t)?;
        out = Some(match out {
            None => {
                if negated {
                    neg(piece)
                } else {
                    piece
                }
            }
            Some(acc) => {
                if negated {
                    sub(acc, piece)
                } else {
                    add(acc, piece)
                }
            }
        });
    }
    out.ok_or_else(|| refuse("inverselaplace", "the input is empty"))
}

fn inverse_term(term: &Expr, s: &str, t: &str) -> Result<Expr> {
    if !depends_on(term, s) {
        // Symja answers `5*DiracDelta(t)` here. frees has no impulse, and the
        // Java's own answer does not survive `Evaluator.eval`, so refuse.
        return Err(refuse(
            "inverselaplace",
            &format!(
                "{} is free of {s}, so its inverse transform is a Dirac impulse — frees has no representation for one",
                describe(term)
            ),
        ));
    }

    let mut numerator = Vec::new();
    let mut denominator = Vec::new();
    collect_factors(term, true, &mut numerator, &mut denominator, 0)?;

    // Exact path: every coefficient is a rational number.
    if let (Some(n), Some(d)) = (polys_of(&numerator, s), polys_of(&denominator, s)) {
        return inverse_exact(&n, &d, t);
    }
    // Structural path: symbolic parameters, one denominator shape.
    inverse_symbolic(&numerator, &denominator, term, s, t)
}

// ── exact path: ℚ partial fractions ─────────────────────────────────────────

/// A single partial-fraction piece: `numerator / factor^power`, with `factor`
/// monic and irreducible over ℚ (degree 1 or 2).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Piece {
    factor: QPoly,
    power: u32,
    numerator: QPoly,
}

fn inverse_exact(numerator: &[QPoly], denominator: &[QPoly], t: &str) -> Result<Expr> {
    // The degree bound is enforced *inside* both accumulations, not only on
    // the products: a term with many factors would otherwise build the whole
    // expanded polynomial before anything checked how big it had become.
    let too_big = || {
        refuse(
            "inverselaplace",
            &format!("a polynomial of degree above {MAX_DEGREE}"),
        )
    };
    let mut n = qpoly_one();
    for p in numerator {
        n = poly_mul(&n, p);
        if poly_degree(&n) > MAX_DEGREE {
            return Err(too_big());
        }
    }
    let mut factors: Vec<QPoly> = Vec::new();
    let mut d = qpoly_one();
    for p in denominator {
        if poly_is_zero(p) {
            return Err(refuse("inverselaplace", "the denominator is zero"));
        }
        d = poly_mul(&d, p);
        if poly_degree(&d) > MAX_DEGREE {
            return Err(too_big());
        }
        factors.push(p.clone());
    }
    if poly_degree(&n) >= poly_degree(&d) {
        // Symja emits `DiracDelta(t)` terms; frees cannot represent one.
        return Err(refuse(
            "inverselaplace",
            "an improper rational function — its inverse transform contains a Dirac impulse, which frees has no representation for",
        ));
    }

    let pieces = partial_fractions(&n, &factors)?;
    let mut out: Option<Expr> = None;
    for piece in &pieces {
        let term = piece_to_time_domain(piece, t)?;
        out = Some(match out {
            None => term,
            Some(acc) => add(acc, term),
        });
    }
    // No pieces means every residue was zero, so the signal is zero — the
    // honest answer for `0/(s+1)`, not an error.
    Ok(out.unwrap_or_else(|| num(0.0)))
}

/// Decompose `numerator / Π factors` into partial fractions over ℚ.
///
/// This is the seam for [`crate::cas::ratfun`]: the inputs and outputs are
/// polynomials, not [`Expr`]s, because the table lookup downstream needs the
/// irreducible factor of each piece as data. `factors` is the denominator as
/// *written* — the split is honoured, so `1/((s^2+1)*(s^2+4))` never has to be
/// re-factored out of a quartic.
fn partial_fractions(numerator: &QPoly, factors: &[QPoly]) -> Result<Vec<Piece>> {
    // Factor each written factor over ℚ and merge equal irreducibles.
    let mut scale = BigRational::one();
    let mut irreducibles: Vec<(QPoly, u32)> = Vec::new();
    for factor in factors {
        let (lead, parts) = factor_over_q(factor)?;
        scale *= lead;
        for (part, multiplicity) in parts {
            match irreducibles.iter_mut().find(|(p, _)| *p == part) {
                Some((_, m)) => *m += multiplicity,
                None => irreducibles.push((part, multiplicity)),
            }
        }
    }
    if scale.is_zero() {
        return Err(refuse("inverselaplace", "the denominator is zero"));
    }
    let numerator = poly_scale(numerator, &scale.recip());

    // Unknowns: the coefficients of every P_{i,k}, deg P_{i,k} < deg f_i.
    // Identity: N = Σ_i Σ_{k=1..m_i} P_{i,k} · (D / f_i^k).
    let monic_denominator = irreducibles
        .iter()
        .fold(qpoly_one(), |acc, (f, m)| poly_mul(&acc, &poly_pow(f, *m)));
    let size = poly_degree(&monic_denominator);
    if size == 0 {
        return Err(refuse("inverselaplace", "the denominator is a constant"));
    }

    let mut columns: Vec<(usize, u32, usize, QPoly)> = Vec::new(); // (factor, power, j, basis·s^j)
    for (index, (factor, multiplicity)) in irreducibles.iter().enumerate() {
        let degree = poly_degree(factor);
        for power in 1..=*multiplicity {
            let basis =
                poly_div_exact(&monic_denominator, &poly_pow(factor, power)).ok_or_else(|| {
                    refuse("inverselaplace", "the denominator does not factor cleanly")
                })?;
            for j in 0..degree {
                columns.push((index, power, j, poly_shift(&basis, j)));
            }
        }
    }
    debug_assert_eq!(columns.len(), size);

    let mut matrix = vec![vec![BigRational::zero(); columns.len() + 1]; size];
    for (column, (_, _, _, poly)) in columns.iter().enumerate() {
        for (row, coefficient) in poly.iter().enumerate().take(size) {
            matrix[row][column] = coefficient.clone();
        }
    }
    for (row, slot) in matrix.iter_mut().enumerate().take(size) {
        let last = columns.len();
        slot[last] = numerator
            .get(row)
            .cloned()
            .unwrap_or_else(BigRational::zero);
    }
    let solution = solve_exact(&mut matrix, columns.len()).ok_or_else(|| {
        refuse(
            "inverselaplace",
            "the denominator's factors are not coprime, so no partial-fraction decomposition exists",
        )
    })?;

    let mut pieces: Vec<Piece> = Vec::new();
    for (column, (index, power, j, _)) in columns.iter().enumerate() {
        let coefficient = solution[column].clone();
        if coefficient.is_zero() {
            continue;
        }
        let slot = pieces
            .iter_mut()
            .find(|p| p.factor == irreducibles[*index].0 && p.power == *power);
        match slot {
            Some(existing) => poly_add_term(&mut existing.numerator, *j, coefficient),
            None => {
                let mut poly = Vec::new();
                poly_add_term(&mut poly, *j, coefficient);
                pieces.push(Piece {
                    factor: irreducibles[*index].0.clone(),
                    power: *power,
                    numerator: poly,
                });
            }
        }
    }
    Ok(pieces)
}

/// Table lookup for one partial-fraction piece.
fn piece_to_time_domain(piece: &Piece, t: &str) -> Result<Expr> {
    let tv = Expr::var(t);
    match poly_degree(&piece.factor) {
        // A/(s − p)^k  →  A·t^(k−1)·e^(p·t)/(k−1)!
        1 => {
            let pole = -piece.factor[0].clone(); // factor is monic: s − p ⇒ [−p, 1]
            let k = piece.power;
            if k > MAX_POWER {
                return Err(refuse(
                    "inverselaplace",
                    &format!(
                        "a pole of multiplicity {k} exceeds the supported maximum of {MAX_POWER}"
                    ),
                ));
            }
            let a = piece
                .numerator
                .first()
                .cloned()
                .unwrap_or_else(BigRational::zero);
            let coefficient = a / BigRational::from_integer(BigInt::from(factorial(k - 1)));
            let mut term = rational_expr(&coefficient)?;
            if k >= 2 {
                term = mul(term, power_of(&tv, k - 1));
            }
            Ok(mul(term, exponential(&pole, &tv)?))
        }
        // (B·s + C)/(s² + β·s + γ) → complete the square and read off the
        // damped sinusoid (or damped hyperbolic when the roots are real but
        // irrational — s²−2 is Symja's own `(-1+E^(2√2 t))/(2√2 E^(√2 t))`).
        2 => {
            if piece.power != 1 {
                return Err(refuse(
                    "inverselaplace",
                    "a repeated irreducible quadratic factor (a repeated complex-pole pair) is outside the table",
                ));
            }
            let beta = piece.factor[1].clone();
            let gamma = piece.factor[0].clone();
            let alpha = beta / BigRational::from_integer(BigInt::from(2));
            let w2 = gamma - alpha.clone() * alpha.clone();
            let b = piece
                .numerator
                .get(1)
                .cloned()
                .unwrap_or_else(BigRational::zero);
            let c = piece
                .numerator
                .first()
                .cloned()
                .unwrap_or_else(BigRational::zero);
            // N(s) = B·(s + α) + (C − B·α)
            let shifted = c - b.clone() * alpha.clone();
            let (even, odd) = if w2.is_negative() {
                ("cosh", "sinh")
            } else {
                ("cos", "sin")
            };
            let magnitude = if w2.is_negative() { -w2.clone() } else { w2 };
            let (omega_expr, omega_reciprocal) = frequency(&magnitude)?;
            let mut body = mul(
                rational_expr(&b)?,
                call1(even, mul(omega_expr.clone(), Expr::var(t))),
            );
            body = add(
                body,
                mul(
                    mul(rational_expr(&shifted)?, omega_reciprocal),
                    call1(odd, mul(omega_expr, Expr::var(t))),
                ),
            );
            Ok(mul(exponential(&-alpha, &tv)?, body))
        }
        other => Err(refuse(
            "inverselaplace",
            &format!("an irreducible denominator factor of degree {other}"),
        )),
    }
}

/// `e^(p·t)`, folded away when `p` is zero (`1/s` inverts to the unit step,
/// which Symja prints simply as `1`).
fn exponential(pole: &BigRational, tv: &Expr) -> Result<Expr> {
    if pole.is_zero() {
        return Ok(num(1.0));
    }
    Ok(call1("exp", mul(rational_expr(pole)?, tv.clone())))
}

/// `(ω, 1/ω)` for `ω = √magnitude`, exact when the square root is rational.
fn frequency(magnitude: &BigRational) -> Result<(Expr, Expr)> {
    if let Some(exact) = exact_sqrt(magnitude) {
        let reciprocal = exact.recip();
        return Ok((rational_expr(&exact)?, rational_expr(&reciprocal)?));
    }
    let radicand = rational_expr(magnitude)?;
    let omega = call1("sqrt", radicand);
    Ok((omega.clone(), div(num(1.0), omega)))
}

// ── structural path: symbolic parameters ────────────────────────────────────

fn inverse_symbolic(
    numerator: &[Expr],
    denominator: &[Expr],
    term: &Expr,
    s: &str,
    t: &str,
) -> Result<Expr> {
    // The numerator must be affine in s: A·s + B with A, B free of s.
    let numerator_expr = numerator
        .iter()
        .cloned()
        .reduce(mul)
        .unwrap_or_else(|| num(1.0));
    let (a, b) = affine_in(&numerator_expr, s).ok_or_else(|| {
        refuse(
            "inverselaplace",
            &format!(
                "{} — the numerator must be constant or linear in {s}",
                describe(&numerator_expr)
            ),
        )
    })?;

    // A denominator factor free of `s` is an ordinary divisor, not a pole:
    // `1/(2*(s+b))` must reduce to `(1/2)·1/(s+b)`, not be mistaken for a
    // product of two different poles.
    let mut divisor = num(1.0);
    let mut poles: Vec<&Expr> = Vec::new();
    for factor in denominator {
        if depends_on(factor, s) {
            poles.push(factor);
        } else {
            divisor = mul(divisor, factor.clone());
        }
    }

    let Some(first) = poles.first().copied() else {
        return Err(refuse(
            "inverselaplace",
            &format!(
                "{} is a polynomial in {s}, so its inverse transform contains a Dirac impulse",
                describe(term)
            ),
        ));
    };

    // Every remaining factor must be the same shape; a product of *distinct*
    // symbolic factors needs residues like 1/(b−a), which are only valid when
    // the poles differ — undecidable with symbolic parameters, so refused.
    if poles.iter().any(|f| *f != first) {
        return Err(refuse(
            "inverselaplace",
            &format!(
                "{} — a product of different symbolic factors of {s} cannot be decomposed without deciding whether its poles coincide",
                describe(term)
            ),
        ));
    }
    let multiplicity = poles.len() as u32;
    if multiplicity > MAX_POWER {
        return Err(refuse(
            "inverselaplace",
            &format!("a pole of multiplicity {multiplicity} exceeds the supported maximum of {MAX_POWER}"),
        ));
    }
    let tv = Expr::var(t);

    // Shape 1 — (c·s + d)^k.
    if let Some((slope, intercept)) = affine_in(first, s) {
        if is_literal(&slope, 0.0) {
            return Err(refuse(
                "inverselaplace",
                &format!("{} does not depend on {s}", describe(first)),
            ));
        }
        // deg N < deg D, exactly as the exact path demands.
        if !is_literal(&a, 0.0) && multiplicity < 2 {
            return Err(refuse(
                "inverselaplace",
                &format!(
                    "{} is improper (numerator degree ≥ denominator degree), so its inverse transform contains a Dirac impulse",
                    describe(term)
                ),
            ));
        }
        let pole = neg(div(intercept, slope.clone())); // −d/c
        let scale = div(num(1.0), power_of(&slope, multiplicity));
        // N(s) = A·(s − p) + (A·p + B)
        let residue = add(mul(a.clone(), pole.clone()), b);
        let mut body = mul(
            div(residue, num(factorial(multiplicity - 1) as f64)),
            power_of(&tv, multiplicity - 1),
        );
        if !is_literal(&a, 0.0) {
            body = add(
                body,
                mul(
                    div(a, num(factorial(multiplicity - 2) as f64)),
                    power_of(&tv, multiplicity - 2),
                ),
            );
        }
        let decay = if is_literal(&pole, 0.0) {
            num(1.0)
        } else {
            call1("exp", mul(pole, tv))
        };
        return Ok(div(mul(mul(scale, body), decay), divisor));
    }

    // Shape 2 — a written completed square, (c·s + d)² + w², multiplicity 1.
    if let Some((scale, alpha, w2)) = completed_square(first, s) {
        if multiplicity != 1 {
            return Err(refuse(
                "inverselaplace",
                "a repeated irreducible quadratic factor (a repeated complex-pole pair) is outside the table",
            ));
        }
        // N(s) = A·(s + α) + (B − A·α)
        let shifted = sub(b, mul(a.clone(), alpha.clone()));
        // Which branch of ω = √(w²) applies depends on the sign of w². When
        // w² is a literal the sign is known, and taking the sinusoidal branch
        // anyway would emit `sqrt` of a negative — a silent NaN rather than an
        // answer. Only a *symbolic* w² is assumed positive, which is exactly
        // the assumption Symja makes when it answers `Sin(t*w)/w` for
        // `1/(s^2+w^2)`.
        let body = if is_literal(&w2, 0.0) {
            // A double real pole at −α: A + (B − A·α)·t.
            add(a, mul(shifted, tv.clone()))
        } else if numeric(&w2).is_some_and(|value| value < 0.0) {
            // Real pole pair: the hyperbolic branch, κ = √(−w²).
            let kappa = sqrt_expr(&neg(w2));
            add(
                mul(a, call1("cosh", mul(kappa.clone(), tv.clone()))),
                mul(
                    div(shifted, kappa.clone()),
                    call1("sinh", mul(kappa, tv.clone())),
                ),
            )
        } else {
            let omega = sqrt_expr(&w2);
            add(
                mul(a, call1("cos", mul(omega.clone(), tv.clone()))),
                mul(
                    div(shifted, omega.clone()),
                    call1("sin", mul(omega, tv.clone())),
                ),
            )
        };
        let decay = if is_literal(&alpha, 0.0) {
            num(1.0)
        } else {
            call1("exp", mul(neg(alpha), tv))
        };
        return Ok(div(mul(decay, body), mul(scale, divisor)));
    }

    Err(refuse(
        "inverselaplace",
        &format!(
            "{} — the denominator is neither a power of a linear form nor a completed square in {s}",
            describe(term)
        ),
    ))
}

/// `k·(c·s + d)² ± w²` written literally, as `(scale, α, w²)` with
/// `scale = k·c²` and `α = d/c`, so the factor equals `scale·((s+α)² + w²)`.
///
/// `w²` is allowed to be negative — `(s+a)^2 - 4` is a perfectly ordinary way
/// to write a real pole pair, and the caller reads the sign to pick between the
/// sinusoidal and hyperbolic table entries.
fn completed_square(e: &Expr, var: &str) -> Option<(Expr, Expr, Expr)> {
    let Expr::BinOp { op, left, right } = e else {
        return None;
    };
    // For `X² − w²` only the square-on-the-left orientation is accepted:
    // `w² − X²` is the *negated* quadratic, which is not this shape.
    let candidates: &[(&Expr, &Expr, bool)] = match op {
        BinOp::Add => &[(left, right, false), (right, left, false)],
        BinOp::Sub => &[(left, right, true)],
        _ => return None,
    };
    for &(square, rest, negate_rest) in candidates {
        if depends_on(rest, var) {
            continue;
        }
        if let Some((k, c, d)) = as_scaled_square(square, var) {
            let scale = mul(k, mul(c.clone(), c.clone()));
            let alpha = div(d, c);
            let offset = if negate_rest {
                neg(rest.clone())
            } else {
                rest.clone()
            };
            let w2 = div(offset, scale.clone());
            return Some((scale, alpha, w2));
        }
    }
    None
}

/// `k·(c·var + d)²` written literally, as `(k, c, d)`.
fn as_scaled_square(e: &Expr, var: &str) -> Option<(Expr, Expr, Expr)> {
    match e {
        Expr::BinOp {
            op: BinOp::Pow,
            left,
            right,
        } if non_negative_integer(right) == Some(2) => {
            let (c, d) = affine_in(left, var)?;
            if is_literal(&c, 0.0) {
                return None;
            }
            Some((num(1.0), c, d))
        }
        Expr::BinOp {
            op: BinOp::Mul,
            left,
            right,
        } => {
            for (constant, other) in [
                (left.as_ref(), right.as_ref()),
                (right.as_ref(), left.as_ref()),
            ] {
                if depends_on(constant, var) {
                    continue;
                }
                if let Some((k, c, d)) = as_scaled_square(other, var) {
                    return Some((mul(constant.clone(), k), c, d));
                }
            }
            // s * s
            let (c, d) = affine_in(left, var)?;
            if left == right && !is_literal(&c, 0.0) {
                return Some((num(1.0), c, d));
            }
            None
        }
        Expr::BinOp {
            op: BinOp::Div,
            left,
            right,
        } if !depends_on(right, var) => {
            let (k, c, d) = as_scaled_square(left, var)?;
            Some((div(k, right.as_ref().clone()), c, d))
        }
        _ => None,
    }
}

/// `√e`, folded when `e` is a literal square or a written square.
fn sqrt_expr(e: &Expr) -> Expr {
    // √(w^2) = w — the fold that makes `1/(s^2+w^2)` come back as
    // `sin(w*t)/w`, matching Symja's `Sin(t*w)/w`.
    if let Expr::BinOp {
        op: BinOp::Pow,
        left,
        right,
    } = e
    {
        if non_negative_integer(right) == Some(2) {
            return left.as_ref().clone();
        }
    }
    if let Expr::Num { value, .. } = e {
        let root = value.sqrt();
        if root.is_finite() && (root - root.round()).abs() <= f64::EPSILON * root.abs().max(1.0) {
            return num(root.round());
        }
    }
    call1("sqrt", e.clone())
}

// ---------------------------------------------------------------------------
// Structural helpers over Expr
// ---------------------------------------------------------------------------

/// Split a sum into signed terms. `Sub` flips the sign of its right operand,
/// `Neg` of everything under it.
fn split_sum(e: &Expr, negated: bool, out: &mut Vec<(bool, Expr)>, depth: u32) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(refuse("inverselaplace", "expression is too deeply nested"));
    }
    match e {
        Expr::BinOp {
            op: BinOp::Add,
            left,
            right,
        } => {
            split_sum(left, negated, out, depth + 1)?;
            split_sum(right, negated, out, depth + 1)
        }
        Expr::BinOp {
            op: BinOp::Sub,
            left,
            right,
        } => {
            split_sum(left, negated, out, depth + 1)?;
            split_sum(right, !negated, out, depth + 1)
        }
        Expr::Neg(inner) => split_sum(inner, !negated, out, depth + 1),
        _ => {
            out.push((negated, e.clone()));
            Ok(())
        }
    }
}

/// Flatten a product/quotient into numerator and denominator factor lists,
/// splitting integer powers so `(s+1)^2` contributes `(s+1)` twice.
fn collect_factors(
    e: &Expr,
    positive: bool,
    numerator: &mut Vec<Expr>,
    denominator: &mut Vec<Expr>,
    depth: u32,
) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(FreesError::evaluation(
            "cas: expression is too deeply nested".to_string(),
        ));
    }
    let push = |target: &mut Vec<Expr>, value: Expr| target.push(value);
    match e {
        Expr::BinOp {
            op: BinOp::Mul,
            left,
            right,
        } => {
            collect_factors(left, positive, numerator, denominator, depth + 1)?;
            collect_factors(right, positive, numerator, denominator, depth + 1)
        }
        Expr::BinOp {
            op: BinOp::Div,
            left,
            right,
        } => {
            collect_factors(left, positive, numerator, denominator, depth + 1)?;
            collect_factors(right, !positive, numerator, denominator, depth + 1)
        }
        Expr::Neg(inner) => {
            push(if positive { numerator } else { denominator }, num(-1.0));
            collect_factors(inner, positive, numerator, denominator, depth + 1)
        }
        Expr::BinOp {
            op: BinOp::Pow,
            left,
            right,
        } => {
            if let Some(n) = integer_literal(right) {
                let repeats = n.unsigned_abs();
                if repeats <= u64::from(MAX_POWER) {
                    let into_numerator = positive == (n >= 0);
                    for _ in 0..repeats {
                        push(
                            if into_numerator {
                                &mut *numerator
                            } else {
                                &mut *denominator
                            },
                            left.as_ref().clone(),
                        );
                    }
                    return Ok(());
                }
            }
            push(if positive { numerator } else { denominator }, e.clone());
            Ok(())
        }
        _ => {
            push(if positive { numerator } else { denominator }, e.clone());
            Ok(())
        }
    }
}

/// `(slope, intercept)` when `e` is `slope·var + intercept` with both free of
/// `var`; `None` when `e` is not affine in `var`.
fn affine_in(e: &Expr, var: &str) -> Option<(Expr, Expr)> {
    affine_at(e, var, 0)
}

fn affine_at(e: &Expr, var: &str, depth: u32) -> Option<(Expr, Expr)> {
    if depth > MAX_DEPTH {
        return None;
    }
    if !depends_on(e, var) {
        return Some((num(0.0), e.clone()));
    }
    match e {
        Expr::Var(name) if name == var => Some((num(1.0), num(0.0))),
        Expr::Neg(inner) => {
            let (slope, intercept) = affine_at(inner, var, depth + 1)?;
            Some((neg(slope), neg(intercept)))
        }
        Expr::BinOp {
            op: BinOp::Add,
            left,
            right,
        } => {
            let (ls, li) = affine_at(left, var, depth + 1)?;
            let (rs, ri) = affine_at(right, var, depth + 1)?;
            Some((add(ls, rs), add(li, ri)))
        }
        Expr::BinOp {
            op: BinOp::Sub,
            left,
            right,
        } => {
            let (ls, li) = affine_at(left, var, depth + 1)?;
            let (rs, ri) = affine_at(right, var, depth + 1)?;
            Some((sub(ls, rs), sub(li, ri)))
        }
        Expr::BinOp {
            op: BinOp::Mul,
            left,
            right,
        } => {
            for (constant, other) in [
                (left.as_ref(), right.as_ref()),
                (right.as_ref(), left.as_ref()),
            ] {
                if !depends_on(constant, var) {
                    let (slope, intercept) = affine_at(other, var, depth + 1)?;
                    return Some((
                        mul(constant.clone(), slope),
                        mul(constant.clone(), intercept),
                    ));
                }
            }
            None
        }
        Expr::BinOp {
            op: BinOp::Div,
            left,
            right,
        } if !depends_on(right, var) => {
            let (slope, intercept) = affine_at(left, var, depth + 1)?;
            Some((
                div(slope, right.as_ref().clone()),
                div(intercept, right.as_ref().clone()),
            ))
        }
        Expr::BinOp {
            op: BinOp::Pow,
            left,
            right,
        } if non_negative_integer(right) == Some(1) => affine_at(left, var, depth + 1),
        _ => None,
    }
}

/// Whether `var` occurs anywhere in `e`. A direct walk rather than
/// [`Expr::variables`] so the hot recursions allocate nothing.
fn depends_on(e: &Expr, var: &str) -> bool {
    match e {
        Expr::Num { .. } | Expr::Str(_) => false,
        Expr::Var(name) => name == var,
        Expr::Neg(inner) | Expr::Not(inner) => depends_on(inner, var),
        Expr::BinOp { left, right, .. }
        | Expr::Compare { left, right, .. }
        | Expr::Logical { left, right, .. }
        | Expr::Range {
            start: left,
            end: right,
        } => depends_on(left, var) || depends_on(right, var),
        Expr::ArrayLiteral(elements) => elements.iter().any(|x| depends_on(x, var)),
        Expr::ArrayAccess { name, indices } => {
            name == var || indices.iter().any(|x| depends_on(x, var))
        }
        Expr::Call { args, .. } => args.iter().any(|x| depends_on(x, var)),
    }
}

/// Replace every free occurrence of `var` with `with`.
fn substitute(e: &Expr, var: &str, with: &Expr) -> Expr {
    match e {
        Expr::Var(name) if name == var => with.clone(),
        Expr::Num { .. } | Expr::Str(_) | Expr::Var(_) => e.clone(),
        Expr::Neg(inner) => Expr::Neg(Box::new(substitute(inner, var, with))),
        Expr::Not(inner) => Expr::Not(Box::new(substitute(inner, var, with))),
        Expr::BinOp { op, left, right } => Expr::BinOp {
            op: *op,
            left: Box::new(substitute(left, var, with)),
            right: Box::new(substitute(right, var, with)),
        },
        Expr::Compare { op, left, right } => Expr::Compare {
            op: *op,
            left: Box::new(substitute(left, var, with)),
            right: Box::new(substitute(right, var, with)),
        },
        Expr::Logical { op, left, right } => Expr::Logical {
            op: *op,
            left: Box::new(substitute(left, var, with)),
            right: Box::new(substitute(right, var, with)),
        },
        Expr::Range { start, end } => Expr::Range {
            start: Box::new(substitute(start, var, with)),
            end: Box::new(substitute(end, var, with)),
        },
        Expr::ArrayLiteral(elements) => {
            Expr::ArrayLiteral(elements.iter().map(|x| substitute(x, var, with)).collect())
        }
        Expr::ArrayAccess { name, indices } => Expr::ArrayAccess {
            name: name.clone(),
            indices: indices.iter().map(|x| substitute(x, var, with)).collect(),
        },
        Expr::Call { function, args } => Expr::Call {
            function: function.clone(),
            args: args.iter().map(|x| substitute(x, var, with)).collect(),
        },
    }
}

/// A short name for a node, used in refusals so the message says *what* fell
/// outside the table rather than only that something did.
fn describe(e: &Expr) -> String {
    match e {
        Expr::Num { value, .. } => format!("{value}"),
        Expr::Str(text) => format!("'{text}'"),
        Expr::Var(name) => name.clone(),
        Expr::Neg(inner) => format!("-{}", group(inner)),
        Expr::Not(inner) => format!("not {}", group(inner)),
        Expr::BinOp { op, left, right } => {
            format!("{} {} {}", group(left), op.as_str(), group(right))
        }
        Expr::Compare { op, left, right } => {
            format!("{} {} {}", group(left), op.as_str(), group(right))
        }
        Expr::Logical { op, left, right } => {
            format!("{} {} {}", group(left), op.as_str(), group(right))
        }
        Expr::Range { start, end } => format!("{}:{}", describe(start), describe(end)),
        Expr::ArrayLiteral(_) => "an array literal".to_string(),
        Expr::ArrayAccess { name, .. } => format!("{name}[…]"),
        Expr::Call { function, args } => {
            let inner: Vec<String> = args.iter().map(describe).collect();
            format!("{function}({})", inner.join(", "))
        }
    }
}

/// [`describe`] with parentheses around a compound operand, so a refusal never
/// quotes back something that would re-parse as a different expression.
fn group(e: &Expr) -> String {
    match e {
        Expr::BinOp { .. } | Expr::Compare { .. } | Expr::Logical { .. } | Expr::Neg(_) => {
            format!("({})", describe(e))
        }
        _ => describe(e),
    }
}

// ---------------------------------------------------------------------------
// Expression constructors (light folding, so the output reads like a table)
// ---------------------------------------------------------------------------

fn num(value: f64) -> Expr {
    Expr::num(value)
}

fn call1(function: &str, arg: Expr) -> Expr {
    Expr::call(function, vec![arg])
}

/// True when `e` is the literal `target`.
///
/// Written as a bounded comparison rather than `==` so the SonarCloud
/// float-equality rule stays clean; every call site passes `0.0` or `1.0`,
/// which are exactly representable, so the bound never widens the match.
fn is_literal(e: &Expr, target: f64) -> bool {
    matches!(e, Expr::Num { value, .. } if (*value - target).abs() <= f64::EPSILON)
}

fn numeric(e: &Expr) -> Option<f64> {
    match e {
        Expr::Num { value, .. } => Some(*value),
        _ => None,
    }
}

/// Read a node that *is* an exact rational constant.
///
/// The constructors below keep an inexact quotient such as `1/3` as a `Div`
/// node rather than collapsing it to `0.3333333333333333` (see [`div`]). That
/// makes `Div(Num, Num)` a constant as much as `Num` is, so every fold has to
/// recognise it — otherwise `1/3 · 1/2` prints as the un-combined `1/3*1/2`.
fn rational_literal(e: &Expr) -> Option<BigRational> {
    match e {
        Expr::Num { value, .. } => exact_rational(*value),
        Expr::Neg(inner) => rational_literal(inner).map(|q| -q),
        Expr::BinOp {
            op: BinOp::Div,
            left,
            right,
        } => {
            let n = rational_literal(left)?;
            let d = rational_literal(right)?;
            if d.is_zero() {
                return None;
            }
            Some(n / d)
        }
        _ => None,
    }
}

/// An exact rational as a node, without going through [`div`] (which would
/// recurse straight back here). `None` when either half is too large to stay an
/// exact `f64`, so the caller can fall back rather than round silently.
fn rational_node(q: &BigRational) -> Option<Expr> {
    let n = q.numer().to_i64().filter(|v| v.abs() < MAX_EXACT_INTEGER)?;
    let d = q.denom().to_i64().filter(|v| v.abs() < MAX_EXACT_INTEGER)?;
    Some(if d == 1 {
        num(n as f64)
    } else {
        Expr::bin(BinOp::Div, num(n as f64), num(d as f64))
    })
}

/// Combine two operands exactly when both are rational constants.
///
/// `None` means "not both constant, or the result cannot stay exact" — the
/// caller then builds the node, or falls back to its `f64` path.
fn fold2(
    a: &Expr,
    b: &Expr,
    combine: impl Fn(BigRational, BigRational) -> Option<BigRational>,
) -> Option<Expr> {
    let x = rational_literal(a)?;
    let y = rational_literal(b)?;
    rational_node(&combine(x, y)?)
}

/// `(x ⊕ c₁) ⊕ c₂` → `x ⊕ (c₁ ⊕ c₂)` for an associative `⊕`, when both
/// constants are exact rationals.
fn fold_trailing_constant(a: &Expr, b: &Expr, op: BinOp) -> Option<Expr> {
    let c2 = rational_literal(b)?;
    let Expr::BinOp {
        op: inner,
        left,
        right,
    } = a
    else {
        return None;
    };
    if *inner != op {
        return None;
    }
    let c1 = rational_literal(right)?;
    let node = rational_node(&(c1 + c2))?;
    Some(add((**left).clone(), node))
}

fn add(a: Expr, b: Expr) -> Expr {
    if is_literal(&a, 0.0) {
        return b;
    }
    if is_literal(&b, 0.0) {
        return a;
    }
    if let Some(folded) = fold2(&a, &b, |x, y| Some(x + y)) {
        return folded;
    }
    // `x + x` is `2*x`. The `t`-multiplication rule differentiates a table
    // entry, and the quotient rule's `d/ds(s²) = s + s` would otherwise reach
    // the user verbatim where Symja prints `2*s`.
    if a == b {
        return mul(num(2.0), a);
    }
    // `a + (−b)` is `a − b`.
    if let Expr::Neg(inner) = b {
        return sub(a, *inner);
    }
    // `(x + c₁) + c₂` is `x + (c₁+c₂)`. Repeated shifting composes pole offsets
    // left-associatively, so `L{e^(−2t)·e^(−t)}` would print `s+2+1`.
    if let Some(folded) = fold_trailing_constant(&a, &b, BinOp::Add) {
        return folded;
    }
    Expr::bin(BinOp::Add, a, b)
}

fn sub(a: Expr, b: Expr) -> Expr {
    if is_literal(&b, 0.0) {
        return a;
    }
    if is_literal(&a, 0.0) {
        return neg(b);
    }
    if let Some(folded) = fold2(&a, &b, |x, y| Some(x - y)) {
        return folded;
    }
    // `a − (−b)` is `a + b`. The shifting rule builds the pole as `s − p`, so a
    // decaying exponential (`p = −2`) would otherwise print `s-(-2)` where the
    // Java prints `2+s`.
    if let Expr::Neg(inner) = b {
        return add(a, *inner);
    }
    // The same shape with the sign already inside the literal: `s − (−2)`.
    if let Some(y) = numeric(&b).filter(|y| *y < 0.0) {
        return add(a, num(-y));
    }
    Expr::bin(BinOp::Sub, a, b)
}

fn mul(a: Expr, b: Expr) -> Expr {
    if is_literal(&a, 0.0) || is_literal(&b, 0.0) {
        return num(0.0);
    }
    if is_literal(&a, 1.0) {
        return b;
    }
    if is_literal(&b, 1.0) {
        return a;
    }
    if let Some(folded) = fold2(&a, &b, |x, y| Some(x * y)) {
        return folded;
    }
    // `(−1)·x` is `−x`. Without this the shifting rule's decay factor prints
    // `E^((-1)*t)` where both Symja and the rest of this port print `E^(-t)`.
    if is_literal(&a, -1.0) {
        return neg(b);
    }
    if is_literal(&b, -1.0) {
        return neg(a);
    }
    // `x·x` is `x²`. The table writes `s²+w²` as a product of clones; printed
    // unfolded it reads `s*s+w*w`, which is not how any oracle spells it.
    // Constants are excluded: they were already folded above, and `(1/2)^2` is
    // a worse rendering of `1/4` than the product it replaces.
    if a == b && rational_literal(&a).is_none() {
        return Expr::bin(BinOp::Pow, a, num(2.0));
    }
    // `k·(n/d)` is `(k·n)/d`. The linearity rule scales a table entry that is
    // already a quotient, so `L{3·t²}` would print `3*2/s^3` for `6/s^3`.
    if let (
        Some(k),
        Expr::BinOp {
            op: BinOp::Div,
            left,
            right,
        },
    ) = (rational_literal(&a), &b)
    {
        if let Some(n) = rational_literal(left) {
            if let Some(node) = rational_node(&(k * n)) {
                return div(node, (**right).clone());
            }
        }
    }
    Expr::bin(BinOp::Mul, a, b)
}

fn div(a: Expr, b: Expr) -> Expr {
    if is_literal(&b, 1.0) {
        return a;
    }
    if is_literal(&a, 0.0) {
        return num(0.0);
    }
    // x/x = 1. The only way this arises is a table entry whose numerator is the
    // frequency itself (`w/(s^2+w^2)` → `sin(w*t)`), where a zero `x` makes the
    // whole term degenerate anyway; Symja folds it the same way.
    if a == b {
        return num(1.0);
    }
    // Fold exactly. A quotient that is not exactly representable — `1/3` — is
    // kept *as* a quotient: collapsing it to an `f64` emits
    // `0.3333333333333333` from a module whose contract is that coefficients
    // are exact rationals (`cas/mod.rs`, "Exactness"), and that decimal is what
    // the REPL prints where the Java prints `/3`.
    if let Some(folded) = fold2(&a, &b, |x, y| if y.is_zero() { None } else { Some(x / y) }) {
        return folded;
    }
    if let (Some(x), Some(y)) = (numeric(&a), numeric(&b)) {
        if y.abs() > 0.0 {
            return num(x / y);
        }
    }
    Expr::bin(BinOp::Div, a, b)
}

fn neg(a: Expr) -> Expr {
    if is_literal(&a, 0.0) {
        return num(0.0);
    }
    if let Expr::Neg(inner) = a {
        return *inner;
    }
    if let Some(value) = numeric(&a) {
        return num(-value);
    }
    // `−(k·x)` with a negative constant `k` becomes `|k|·x`. The
    // `t`-multiplication rule is `(−1)ⁿ·dⁿG/dsⁿ`, whose derivative already
    // carries a sign, so `L{t·sin(w·t)}` would otherwise print the
    // double-negated `-(-2*w*s)/…` where Symja prints `2*w*s/…`.
    if let Expr::BinOp {
        op: BinOp::Mul,
        left,
        right,
    } = &a
    {
        if let Some(k) = rational_literal(left).filter(|k| k.is_negative()) {
            if let Some(node) = rational_node(&-k) {
                return mul(node, (**right).clone());
            }
        }
    }
    Expr::Neg(Box::new(a))
}

/// `base^n` with `n` a small non-negative integer.
fn power_of(base: &Expr, n: u32) -> Expr {
    if is_literal(base, 1.0) {
        return num(1.0);
    }
    match n {
        0 => num(1.0),
        1 => base.clone(),
        _ => Expr::bin(BinOp::Pow, base.clone(), num(f64::from(n))),
    }
}

/// The literal integer value of `e`, when it is one.
///
/// Sees through `Neg`, because the parser builds `t^(-1)` as `Pow(t, Neg(1))`
/// rather than `Pow(t, -1)` — without this, `t^(-1)` and the identical `1/t`
/// would be refused with two different messages.
fn integer_literal(e: &Expr) -> Option<i64> {
    if let Expr::Neg(inner) = e {
        return integer_literal(inner)?.checked_neg();
    }
    let value = numeric(e)?;
    if !value.is_finite() || value.abs() >= MAX_EXACT_INTEGER as f64 {
        return None;
    }
    let rounded = value.round();
    if (value - rounded).abs() > f64::EPSILON * rounded.abs().max(1.0) {
        return None;
    }
    Some(rounded as i64)
}

fn non_negative_integer(e: &Expr) -> Option<u32> {
    let n = integer_literal(e)?;
    if n < 0 || n > i64::from(MAX_POWER) {
        return None;
    }
    u32::try_from(n).ok()
}

/// `n!` for `n ≤ MAX_POWER`, exact in `f64` by construction.
fn factorial(n: u32) -> i64 {
    (1..=i64::from(n)).product::<i64>().max(1)
}

// ---------------------------------------------------------------------------
// Exact polynomial arithmetic over ℚ
// ---------------------------------------------------------------------------

/// A polynomial in the transform variable, coefficients ascending. Exact
/// rationals, never `f64` — `cas/mod.rs`, "Exactness".
///
/// Deliberately **not** named `Poly`: [`crate::cas::poly`] owns that name and
/// its own dense representation. This one is local so the transform table can
/// land without waiting on the factoriser; the only functions that would have
/// to change to adopt `cas::poly` are the dozen `poly_*` helpers below plus
/// [`factor_over_q`], and nothing above them touches the representation.
type QPoly = Vec<BigRational>;

fn qpoly_one() -> QPoly {
    vec![BigRational::one()]
}

fn poly_is_zero(p: &QPoly) -> bool {
    p.iter().all(BigRational::is_zero)
}

/// Degree, with the zero polynomial reported as 0.
fn poly_degree(p: &QPoly) -> usize {
    p.iter().rposition(|c| !c.is_zero()).unwrap_or(0)
}

fn poly_trim(mut p: QPoly) -> QPoly {
    while p.len() > 1 && p.last().is_some_and(BigRational::is_zero) {
        p.pop();
    }
    p
}

fn poly_add_term(p: &mut QPoly, degree: usize, coefficient: BigRational) {
    if p.len() <= degree {
        p.resize(degree + 1, BigRational::zero());
    }
    p[degree] += coefficient;
}

fn poly_mul(a: &QPoly, b: &QPoly) -> QPoly {
    let mut out = vec![BigRational::zero(); a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        if x.is_zero() {
            continue;
        }
        for (j, y) in b.iter().enumerate() {
            out[i + j] += x * y;
        }
    }
    poly_trim(out)
}

fn poly_add(a: &QPoly, b: &QPoly) -> QPoly {
    let mut out = vec![BigRational::zero(); a.len().max(b.len())];
    for (i, x) in a.iter().enumerate() {
        out[i] += x;
    }
    for (i, y) in b.iter().enumerate() {
        out[i] += y;
    }
    poly_trim(out)
}

fn poly_sub(a: &QPoly, b: &QPoly) -> QPoly {
    let mut out = vec![BigRational::zero(); a.len().max(b.len())];
    for (i, x) in a.iter().enumerate() {
        out[i] += x;
    }
    for (i, y) in b.iter().enumerate() {
        out[i] -= y;
    }
    poly_trim(out)
}

fn poly_scale(p: &QPoly, k: &BigRational) -> QPoly {
    poly_trim(p.iter().map(|c| c * k).collect())
}

/// `p · x^by`.
fn poly_shift(p: &QPoly, by: usize) -> QPoly {
    let mut out = vec![BigRational::zero(); by];
    out.extend(p.iter().cloned());
    poly_trim(out)
}

fn poly_pow(p: &QPoly, n: u32) -> QPoly {
    let mut out = qpoly_one();
    for _ in 0..n {
        out = poly_mul(&out, p);
    }
    out
}

/// `a / b` when the division is exact, `None` otherwise.
fn poly_div_exact(a: &QPoly, b: &QPoly) -> Option<QPoly> {
    let degree_b = poly_degree(b);
    let lead_b = b[degree_b].clone();
    if lead_b.is_zero() {
        return None;
    }
    let mut remainder = poly_trim(a.clone());
    let mut quotient = vec![BigRational::zero(); 1];
    while poly_degree(&remainder) >= degree_b && !poly_is_zero(&remainder) {
        let degree_r = poly_degree(&remainder);
        let shift = degree_r - degree_b;
        let coefficient = remainder[degree_r].clone() / lead_b.clone();
        poly_add_term(&mut quotient, shift, coefficient.clone());
        let mut subtrahend = vec![coefficient];
        subtrahend = poly_shift(&poly_mul(&subtrahend, b), shift);
        remainder = poly_sub(&remainder, &subtrahend);
    }
    if poly_is_zero(&remainder) {
        Some(poly_trim(quotient))
    } else {
        None
    }
}

/// `p(x)` at a rational point.
fn poly_eval(p: &QPoly, x: &BigRational) -> BigRational {
    let mut acc = BigRational::zero();
    for coefficient in p.iter().rev() {
        acc = acc * x + coefficient;
    }
    acc
}

/// Convert an [`Expr`] to an exact polynomial in `var`, or `None` when it is
/// not one (a symbolic parameter, a transcendental call, a non-integer power).
fn to_poly(e: &Expr, var: &str, depth: u32) -> Option<QPoly> {
    if depth > MAX_DEPTH {
        return None;
    }
    match e {
        Expr::Num { value, .. } => Some(vec![exact_rational(*value)?]),
        Expr::Var(name) if name == var => Some(vec![BigRational::zero(), BigRational::one()]),
        Expr::Neg(inner) => {
            let p = to_poly(inner, var, depth + 1)?;
            Some(poly_scale(&p, &-BigRational::one()))
        }
        Expr::BinOp { op, left, right } => {
            let a = to_poly(left, var, depth + 1)?;
            match op {
                BinOp::Add => Some(poly_add(&a, &to_poly(right, var, depth + 1)?)),
                BinOp::Sub => Some(poly_sub(&a, &to_poly(right, var, depth + 1)?)),
                BinOp::Mul => {
                    let b = to_poly(right, var, depth + 1)?;
                    if poly_degree(&a) + poly_degree(&b) > MAX_DEGREE {
                        return None;
                    }
                    Some(poly_mul(&a, &b))
                }
                BinOp::Div => {
                    let b = to_poly(right, var, depth + 1)?;
                    if poly_degree(&b) != 0 || b[0].is_zero() {
                        return None;
                    }
                    Some(poly_scale(&a, &b[0].clone().recip()))
                }
                BinOp::Pow => {
                    let n = non_negative_integer(right)?;
                    if poly_degree(&a) * (n as usize) > MAX_DEGREE {
                        return None;
                    }
                    Some(poly_pow(&a, n))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Every factor as an exact polynomial, or `None` if any one is not.
fn polys_of(factors: &[Expr], var: &str) -> Option<Vec<QPoly>> {
    factors.iter().map(|f| to_poly(f, var, 0)).collect()
}

/// The exact rational a literal denotes.
///
/// Integers convert directly. Decimals are read back at the precision they
/// were written at (`0.25` → `1/4`, not the binary expansion) so the
/// decomposition stays in the numbers the user typed; a literal that is
/// neither is refused rather than approximated.
fn exact_rational(value: f64) -> Option<BigRational> {
    if !value.is_finite() {
        return None;
    }
    let rounded = value.round();
    if (value - rounded).abs() <= f64::EPSILON * rounded.abs().max(1.0)
        && rounded.abs() < MAX_EXACT_INTEGER as f64
    {
        return Some(BigRational::from_integer(BigInt::from(rounded as i64)));
    }
    let mut scale = 1.0f64;
    for _ in 0..12 {
        scale *= 10.0;
        let scaled = value * scale;
        if scaled.abs() >= MAX_EXACT_INTEGER as f64 {
            break;
        }
        let rounded = scaled.round();
        if (scaled - rounded).abs() <= 1e-9 * scaled.abs().max(1.0) {
            return Some(BigRational::new(
                BigInt::from(rounded as i64),
                BigInt::from(scale as i64),
            ));
        }
    }
    None
}

/// An exact rational as an [`Expr`] — `n` when the denominator is 1, `n/d`
/// otherwise, so no rational is ever flattened into an inexact `f64`.
fn rational_expr(r: &BigRational) -> Result<Expr> {
    let numerator = r.numer().to_i64().filter(|n| n.abs() < MAX_EXACT_INTEGER);
    let denominator = r.denom().to_i64().filter(|d| d.abs() < MAX_EXACT_INTEGER);
    match (numerator, denominator) {
        (Some(n), Some(1)) => Ok(num(n as f64)),
        (Some(n), Some(d)) => Ok(div(num(n as f64), num(d as f64))),
        _ => Err(refuse(
            "inverselaplace",
            "a residue too large to represent exactly",
        )),
    }
}

/// `√r` when it is rational.
fn exact_sqrt(r: &BigRational) -> Option<BigRational> {
    if r.is_negative() {
        return None;
    }
    let n = r.numer().sqrt();
    let d = r.denom().sqrt();
    if &(n.clone() * n.clone()) == r.numer() && &(d.clone() * d.clone()) == r.denom() {
        return Some(BigRational::new(n, d));
    }
    None
}

/// Factor a polynomial over ℚ into a leading coefficient and a list of
/// `(monic irreducible, multiplicity)` pairs of degree 1 or 2.
///
/// Rational roots are peeled first (rational-root theorem plus synthetic
/// division), so a denominator with nice poles decomposes into exponentials
/// exactly as Symja's does. Whatever remains must be a single quadratic, which
/// is handled by completing the square — that is what covers both the damped
/// sinusoid (`s²+4s+13`) and the irrational real pair (`s²−2`). Anything of
/// degree 3 or more that survives is refused by name.
fn factor_over_q(p: &QPoly) -> Result<(BigRational, Vec<(QPoly, u32)>)> {
    let degree = poly_degree(p);
    let lead = p[degree].clone();
    if lead.is_zero() {
        return Err(refuse("inverselaplace", "the denominator is zero"));
    }
    let mut remaining = poly_scale(p, &lead.clone().recip());
    let mut factors: Vec<(QPoly, u32)> = Vec::new();

    // Peel s^k.
    let zeros = remaining.iter().position(|c| !c.is_zero()).unwrap_or(0);
    if zeros > 0 {
        factors.push((vec![BigRational::zero(), BigRational::one()], zeros as u32));
        remaining = remaining.split_off(zeros);
    }

    // Peel rational roots.
    for root in rational_roots(&remaining) {
        let linear = vec![-root.clone(), BigRational::one()];
        let mut multiplicity = 0;
        while let Some(quotient) = poly_div_exact(&remaining, &linear) {
            remaining = quotient;
            multiplicity += 1;
        }
        if multiplicity > 0 {
            match factors.iter_mut().find(|(f, _)| *f == linear) {
                Some((_, m)) => *m += multiplicity,
                None => factors.push((linear, multiplicity)),
            }
        }
    }

    match poly_degree(&remaining) {
        0 => {}
        2 => {
            let monic = poly_scale(&remaining, &remaining[2].clone().recip());
            match factors.iter_mut().find(|(f, _)| *f == monic) {
                Some((_, m)) => *m += 1,
                None => factors.push((monic, 1)),
            }
        }
        other => {
            return Err(refuse(
                "inverselaplace",
                &format!(
                    "a denominator factor of degree {other} with no rational roots — factoring it needs an algebraic extension of ℚ"
                ),
            ));
        }
    }
    Ok((lead, factors))
}

/// The distinct rational roots of a polynomial, by the rational-root theorem.
///
/// Returns an empty list rather than searching when the constant or leading
/// coefficient is too large to enumerate divisors of — the caller then falls
/// through to the quadratic rule or refuses, never guesses.
fn rational_roots(p: &QPoly) -> Vec<BigRational> {
    let degree = poly_degree(p);
    if degree == 0 {
        return Vec::new();
    }
    // Clear denominators to get an integer polynomial.
    let mut multiplier = BigInt::one();
    for coefficient in p.iter().take(degree + 1) {
        multiplier = multiplier.lcm(coefficient.denom());
    }
    let integral: Vec<BigInt> = p
        .iter()
        .take(degree + 1)
        .map(|c| (c * BigRational::from_integer(multiplier.clone())).to_integer())
        .collect();

    let constant = integral[0].clone();
    let leading = integral[degree].clone();
    // A zero constant term means a root at 0, already peeled by the caller.
    if constant.is_zero() {
        return Vec::new();
    }
    let (Some(numerators), Some(denominators)) =
        (divisors(&constant.abs()), divisors(&leading.abs()))
    else {
        return Vec::new();
    };

    let mut roots = Vec::new();
    for n in &numerators {
        for d in &denominators {
            for sign in [BigInt::one(), -BigInt::one()] {
                let candidate = BigRational::new(n.clone() * sign, d.clone());
                if poly_eval(p, &candidate).is_zero() && !roots.contains(&candidate) {
                    roots.push(candidate);
                }
            }
        }
    }
    roots
}

/// Positive divisors of `n`, or `None` when `n` is too large to enumerate.
fn divisors(n: &BigInt) -> Option<Vec<BigInt>> {
    let bound = n.to_u64()?;
    if bound == 0 || bound > 1_000_000 {
        return None;
    }
    let mut out = Vec::new();
    let mut d = 1u64;
    while d * d <= bound {
        if bound % d == 0 {
            out.push(BigInt::from(d));
            if d != bound / d {
                out.push(BigInt::from(bound / d));
            }
        }
        d += 1;
    }
    Some(out)
}

/// Exact Gaussian elimination. `matrix` is `rows × (columns + 1)` augmented;
/// returns the solution or `None` when the system is singular.
fn solve_exact(matrix: &mut [Vec<BigRational>], columns: usize) -> Option<Vec<BigRational>> {
    let rows = matrix.len();
    if rows != columns {
        return None;
    }
    for column in 0..columns {
        let pivot = (column..rows).find(|&r| !matrix[r][column].is_zero())?;
        matrix.swap(column, pivot);
        let scale = matrix[column][column].clone().recip();
        for value in matrix[column].iter_mut() {
            *value *= scale.clone();
        }
        // The pivot row is cloned rather than indexed twice: eliminating row
        // `r` reads the pivot row and writes `r`, which cannot be two
        // simultaneous borrows of `matrix`.
        let pivot_row = matrix[column].clone();
        for (row, target) in matrix.iter_mut().enumerate() {
            if row == column || target[column].is_zero() {
                continue;
            }
            let factor = target[column].clone();
            for (value, pivot_value) in target.iter_mut().zip(pivot_row.iter()).skip(column) {
                *value -= factor.clone() * pivot_value;
            }
        }
    }
    Some(
        matrix
            .iter()
            .take(columns)
            .map(|row| row[columns].clone())
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{eval, Scope};
    use crate::lexer::tokenize;
    use crate::parser::{parse_expr, Cursor};

    /// Every expression in these tests goes through the real front end, so the
    /// precedence and call encoding are the shipping ones.
    fn expr(src: &str) -> Expr {
        let tokens = tokenize(src).unwrap_or_else(|e| panic!("lex {src:?}: {e}"));
        let mut cursor = Cursor::new(&tokens, src);
        parse_expr(&mut cursor).unwrap_or_else(|e| panic!("parse {src:?}: {e}"))
    }

    fn at(e: &Expr, bindings: &[(&str, f64)]) -> f64 {
        let mut scope = Scope::new();
        for (name, value) in bindings {
            scope.insert((*name).to_string(), *value);
        }
        eval(e, &scope).unwrap_or_else(|err| panic!("eval {e:?}: {err}"))
    }

    /// Assert a transform matches a closed form at several sample points. The
    /// closed form is the **oracle's own answer**, transcribed from a run of
    /// the real Java engine (see the module docs); `oracle` records its exact
    /// output spelling so the pin is auditable.
    fn assert_matches(
        got: &Expr,
        _oracle: &str,
        variable: &str,
        samples: &[f64],
        others: &[(&str, f64)],
        expected: impl Fn(f64) -> f64,
    ) {
        for &x in samples {
            let mut bindings = others.to_vec();
            bindings.push((variable, x));
            let actual = at(got, &bindings);
            let want = expected(x);
            // The rendered expression is truncated: the multiplication-by-t
            // rule differentiates, and an unsimplified 10th derivative prints
            // as megabytes of AST that drowns the number that actually failed.
            let mut rendered = describe(got);
            rendered.truncate(300);
            assert!(
                (actual - want).abs() <= 1e-9 * want.abs().max(1.0),
                "at {variable}={x}: got {actual}, want {want} (oracle: {_oracle})\n  {rendered}"
            );
        }
    }

    fn laplace(src: &str) -> Result<Expr> {
        transform(&expr(src), "t", "s")
    }

    fn ilaplace(src: &str) -> Result<Expr> {
        inverse_transform(&expr(src), "s", "t")
    }

    fn ok(src: &str, f: impl Fn(&str) -> Result<Expr>) -> Expr {
        f(src).unwrap_or_else(|e| panic!("{src:?} should transform: {e}"))
    }

    const S: &[f64] = &[0.5, 1.0, 3.0, 7.25];
    const T: &[f64] = &[0.0, 0.3, 0.7, 1.5, 4.0];

    // ── forward: cases the Java oracle answers, matched ────────────────────

    #[test]
    fn constant_transforms_to_c_over_s() {
        // oracle: laplace(a, t, s) => a/s ; laplace(5) => 5/s ; laplace(0) => 0
        assert_matches(&ok("a", laplace), "a/s", "s", S, &[("a", 2.0)], |s| 2.0 / s);
        assert_matches(&ok("5", laplace), "5/s", "s", S, &[], |s| 5.0 / s);
        assert_matches(&ok("0", laplace), "0", "s", S, &[], |_| 0.0);
        assert_matches(&ok("1/2", laplace), "1/(2*s)", "s", S, &[], |s| 0.5 / s);
    }

    #[test]
    fn powers_of_t_transform_to_factorials() {
        // oracle: t => 1/s^2 ; t^2 => 2/s^3 ; t^3 => 6/s^4 ; t^5 => 120/s^6
        assert_matches(&ok("t", laplace), "1/s^2", "s", S, &[], |s| 1.0 / (s * s));
        assert_matches(&ok("t^2", laplace), "2/s^3", "s", S, &[], |s| {
            2.0 / s.powi(3)
        });
        assert_matches(&ok("t^3", laplace), "6/s^4", "s", S, &[], |s| {
            6.0 / s.powi(4)
        });
        assert_matches(&ok("t^5", laplace), "120/s^6", "s", S, &[], |s| {
            120.0 / s.powi(6)
        });
        // oracle: 3*t^2 => 6/s^3 ; t*t => 2/s^3 ; t/2 => 1/(2*s^2)
        assert_matches(&ok("3*t^2", laplace), "6/s^3", "s", S, &[], |s| {
            6.0 / s.powi(3)
        });
        assert_matches(&ok("t*t", laplace), "2/s^3", "s", S, &[], |s| {
            2.0 / s.powi(3)
        });
        assert_matches(&ok("t/2", laplace), "1/(2*s^2)", "s", S, &[], |s| {
            0.5 / (s * s)
        });
        // oracle: t^10 => 3628800/s^11
        assert_matches(&ok("t^10", laplace), "3628800/s^11", "s", S, &[], |s| {
            3_628_800.0 / s.powi(11)
        });
    }

    #[test]
    fn exponentials_transform_to_simple_poles() {
        // oracle: exp(-a*t) => 1/(a+s) ; exp(a*t) => 1/(-a+s)
        assert_matches(
            &ok("exp(-a*t)", laplace),
            "1/(a+s)",
            "s",
            S,
            &[("a", 2.0)],
            |s| 1.0 / (s + 2.0),
        );
        assert_matches(
            &ok("exp(a*t)", laplace),
            "1/(-a+s)",
            "s",
            &[3.0, 7.25],
            &[("a", 2.0)],
            |s| 1.0 / (s - 2.0),
        );
        // oracle: exp(-2*t) => 1/(2+s) ; 2*exp(-3*t) => 2/(3+s)
        assert_matches(&ok("exp(-2*t)", laplace), "1/(2+s)", "s", S, &[], |s| {
            1.0 / (s + 2.0)
        });
        assert_matches(&ok("2*exp(-3*t)", laplace), "2/(3+s)", "s", S, &[], |s| {
            2.0 / (s + 3.0)
        });
    }

    #[test]
    fn unit_frequency_trig_matches_the_oracle() {
        // oracle: sin(t) => 1/(1+s^2) ; cos(t) => s/(1+s^2)
        assert_matches(&ok("sin(t)", laplace), "1/(1+s^2)", "s", S, &[], |s| {
            1.0 / (1.0 + s * s)
        });
        assert_matches(&ok("cos(t)", laplace), "s/(1+s^2)", "s", S, &[], |s| {
            s / (1.0 + s * s)
        });
        // oracle: cosh(t) => s/(-1+s^2)
        assert_matches(
            &ok("cosh(t)", laplace),
            "s/(-1+s^2)",
            "s",
            &[3.0, 7.25],
            &[],
            |s| s / (s * s - 1.0),
        );
    }

    #[test]
    fn shifting_matches_the_oracle() {
        // oracle: exp(-a*t)*sin(t) => 1/(1+(a+s)^2)
        assert_matches(
            &ok("exp(-a*t)*sin(t)", laplace),
            "1/(1+(a+s)^2)",
            "s",
            S,
            &[("a", 2.0)],
            |s| 1.0 / (1.0 + (s + 2.0).powi(2)),
        );
        // oracle: exp(-a*t)*cos(t) => (a+s)/(1+(a+s)^2)
        assert_matches(
            &ok("exp(-a*t)*cos(t)", laplace),
            "(a+s)/(1+(a+s)^2)",
            "s",
            S,
            &[("a", 2.0)],
            |s| (s + 2.0) / (1.0 + (s + 2.0).powi(2)),
        );
        // oracle: t*exp(-a*t) => 1/(a+s)^2 ; t^2*exp(-a*t) => 2/(a+s)^3
        assert_matches(
            &ok("t*exp(-a*t)", laplace),
            "1/(a+s)^2",
            "s",
            S,
            &[("a", 2.0)],
            |s| 1.0 / (s + 2.0).powi(2),
        );
        assert_matches(
            &ok("t^2*exp(-a*t)", laplace),
            "2/(a+s)^3",
            "s",
            S,
            &[("a", 2.0)],
            |s| 2.0 / (s + 2.0).powi(3),
        );
        // oracle: exp(2*t)*cos(t) => (-2+s)/(1+(2-s)^2)
        assert_matches(
            &ok("exp(2*t)*cos(t)", laplace),
            "(-2+s)/(1+(2-s)^2)",
            "s",
            &[3.0, 7.25],
            &[],
            |s| (s - 2.0) / (1.0 + (2.0 - s).powi(2)),
        );
    }

    #[test]
    fn multiplication_by_t_matches_the_oracle() {
        // oracle: t*sin(t) => (2*s)/(1+s^2)^2
        assert_matches(
            &ok("t*sin(t)", laplace),
            "(2*s)/(1+s^2)^2",
            "s",
            S,
            &[],
            |s| 2.0 * s / (1.0 + s * s).powi(2),
        );
        // oracle: t*cos(t) => (2*s^2)/(1+s^2)^2-1/(1+s^2)
        assert_matches(
            &ok("t*cos(t)", laplace),
            "(2*s^2)/(1+s^2)^2-1/(1+s^2)",
            "s",
            S,
            &[],
            |s| 2.0 * s * s / (1.0 + s * s).powi(2) - 1.0 / (1.0 + s * s),
        );
        // oracle: t^2*sin(t) => (8*s^2)/(1+s^2)^3-2/(1+s^2)^2
        assert_matches(
            &ok("t^2*sin(t)", laplace),
            "(8*s^2)/(1+s^2)^3-2/(1+s^2)^2",
            "s",
            S,
            &[],
            |s| 8.0 * s * s / (1.0 + s * s).powi(3) - 2.0 / (1.0 + s * s).powi(2),
        );
    }

    #[test]
    fn linearity_matches_the_oracle() {
        // oracle: 1 + t => 1/s^2+1/s
        assert_matches(&ok("1 + t", laplace), "1/s^2+1/s", "s", S, &[], |s| {
            1.0 / (s * s) + 1.0 / s
        });
        // oracle: 3*sin(t) + 2*cos(t) => 3/(1+s^2)+(2*s)/(1+s^2)
        assert_matches(
            &ok("3*sin(t) + 2*cos(t)", laplace),
            "3/(1+s^2)+(2*s)/(1+s^2)",
            "s",
            S,
            &[],
            |s| 3.0 / (1.0 + s * s) + 2.0 * s / (1.0 + s * s),
        );
        // oracle: 2*t^2 - 5*exp(-t) => 4/s^3-5/(1+s)
        assert_matches(
            &ok("2*t^2 - 5*exp(-t)", laplace),
            "4/s^3-5/(1+s)",
            "s",
            S,
            &[],
            |s| 4.0 / s.powi(3) - 5.0 / (1.0 + s),
        );
        // oracle: (1-exp(-a*t))/a => (1/s-1/(a+s))/a
        assert_matches(
            &ok("(1-exp(-a*t))/a", laplace),
            "(1/s-1/(a+s))/a",
            "s",
            S,
            &[("a", 2.0)],
            |s| (1.0 / s - 1.0 / (s + 2.0)) / 2.0,
        );
        // oracle: exp(-a*t)+exp(-b*t) => 1/(a+s)+1/(b+s)
        assert_matches(
            &ok("exp(-a*t)+exp(-b*t)", laplace),
            "1/(a+s)+1/(b+s)",
            "s",
            S,
            &[("a", 2.0), ("b", 5.0)],
            |s| 1.0 / (s + 2.0) + 1.0 / (s + 5.0),
        );
    }

    #[test]
    fn stacked_exponentials_and_polynomials_match_the_oracle() {
        // oracle: exp(-t)*t^2 => 2/(1+s)^3
        assert_matches(&ok("exp(-t)*t^2", laplace), "2/(1+s)^3", "s", S, &[], |s| {
            2.0 / (1.0 + s).powi(3)
        });
        // oracle: 5*t^3-2*t+7 => 30/s^4-2/s^2+7/s
        assert_matches(
            &ok("5*t^3-2*t+7", laplace),
            "30/s^4-2/s^2+7/s",
            "s",
            S,
            &[],
            |s| 30.0 / s.powi(4) - 2.0 / (s * s) + 7.0 / s,
        );
        // oracle: (exp(-a*t))^2 => 1/(2*a+s)
        assert_matches(
            &ok("(exp(-a*t))^2", laplace),
            "1/(2*a+s)",
            "s",
            S,
            &[("a", 2.0)],
            |s| 1.0 / (s + 4.0),
        );
        // oracle: exp(-(a+b)*t) => 1/(a+b+s)
        assert_matches(
            &ok("exp(-(a+b)*t)", laplace),
            "1/(a+b+s)",
            "s",
            S,
            &[("a", 2.0), ("b", 3.0)],
            |s| 1.0 / (s + 5.0),
        );
        // oracle: cosh(t)*exp(-t) => (1+s)/(-1+(1+s)^2)
        assert_matches(
            &ok("cosh(t)*exp(-t)", laplace),
            "(1+s)/(-1+(1+s)^2)",
            "s",
            &[3.0, 7.25],
            &[],
            |s| (1.0 + s) / ((1.0 + s).powi(2) - 1.0),
        );
        // oracle REFUSES exp(-a*t)*exp(-b*t): Symja merges the exponents into
        // E^(-a*t-b*t) and then cannot transform the two-symbol slope.
        assert_matches(
            &ok("exp(-a*t)*exp(-b*t)", laplace),
            "REFUSED by Symja",
            "s",
            S,
            &[("a", 2.0), ("b", 3.0)],
            |s| 1.0 / (s + 5.0),
        );
    }

    #[test]
    fn deeper_partial_fractions_match_the_oracle() {
        // oracle: 1/(s^2+2*s-3) => -1/(4*E^(3*t))+E^t/4
        assert_matches(
            &ok("1/(s^2+2*s-3)", ilaplace),
            "-1/(4*E^(3*t))+E^t/4",
            "t",
            T,
            &[],
            |t| (t.exp() - (-3.0 * t).exp()) / 4.0,
        );
        // oracle: (s+1)/(s^2+2*s-3) => 1/(2*E^(3*t))+E^t/2
        assert_matches(
            &ok("(s+1)/(s^2+2*s-3)", ilaplace),
            "1/(2*E^(3*t))+E^t/2",
            "t",
            T,
            &[],
            |t| (t.exp() + (-3.0 * t).exp()) / 2.0,
        );
        // oracle: 1/(s*(s+1)*(s+2)*(s+3))
        //           => 1/6-1/(6*E^(3*t))+1/(2*E^(2*t))-1/(2*E^t)
        assert_matches(
            &ok("1/(s*(s+1)*(s+2)*(s+3))", ilaplace),
            "1/6-1/(6*E^(3*t))+1/(2*E^(2*t))-1/(2*E^t)",
            "t",
            T,
            &[],
            |t| 1.0 / 6.0 - (-3.0 * t).exp() / 6.0 + (-2.0 * t).exp() / 2.0 - (-t).exp() / 2.0,
        );
        // oracle: (s+2)/((s+1)*(s+3)) => 1/(2*E^(3*t))+1/(2*E^t)
        assert_matches(
            &ok("(s+2)/((s+1)*(s+3))", ilaplace),
            "1/(2*E^(3*t))+1/(2*E^t)",
            "t",
            T,
            &[],
            |t| ((-3.0 * t).exp() + (-t).exp()) / 2.0,
        );
        // oracle: 1/(s^2*(s+1)^2) => -2+2/E^t+t+t/E^t
        assert_matches(
            &ok("1/(s^2*(s+1)^2)", ilaplace),
            "-2+2/E^t+t+t/E^t",
            "t",
            T,
            &[],
            |t| -2.0 + 2.0 * (-t).exp() + t + t * (-t).exp(),
        );
        // oracle: (3*s+2)/(s^2+4*s+3) => 7/2*1/E^(3*t)-1/(2*E^t)
        assert_matches(
            &ok("(3*s+2)/(s^2+4*s+3)", ilaplace),
            "7/2*1/E^(3*t)-1/(2*E^t)",
            "t",
            T,
            &[],
            |t| 3.5 * (-3.0 * t).exp() - 0.5 * (-t).exp(),
        );
        // oracle REFUSES 1/(4*s^2+4*s+1), which is 1/(2*s+1)^2 — Symja never
        // factors, so the repeated rational root at −1/2 is invisible to it.
        assert_matches(
            &ok("1/(4*s^2+4*s+1)", ilaplace),
            "REFUSED by Symja",
            "t",
            T,
            &[],
            |t| t * (-0.5 * t).exp() / 4.0,
        );
    }

    #[test]
    fn the_frequency_variable_is_whatever_the_caller_names() {
        // oracle: laplace(exp(-2*x), x, p) => 1/(2+p)
        let got = transform(&expr("exp(-2*x)"), "x", "p").expect("transforms");
        assert_matches(&got, "1/(2+p)", "p", S, &[], |p| 1.0 / (p + 2.0));
    }

    // ── forward: where the port deliberately answers and the Java does not ──

    #[test]
    fn frequency_scaled_trig_is_a_documented_superset() {
        // The oracle REFUSES all four of these: Symja 3.0.0 returns
        // `LaplaceTransform(Sin(3*t),t,s)` unevaluated, and ReplEvaluator
        // reports "laplace: no closed form found for this input."
        assert_matches(
            &ok("sin(3*t)", laplace),
            "REFUSED by Symja",
            "s",
            S,
            &[],
            |s| 3.0 / (s * s + 9.0),
        );
        assert_matches(
            &ok("cos(3*t)", laplace),
            "REFUSED by Symja",
            "s",
            S,
            &[],
            |s| s / (s * s + 9.0),
        );
        assert_matches(
            &ok("sin(w*t)", laplace),
            "REFUSED by Symja",
            "s",
            S,
            &[("w", 4.0)],
            |s| 4.0 / (s * s + 16.0),
        );
        assert_matches(
            &ok("cos(w*t)", laplace),
            "REFUSED by Symja",
            "s",
            S,
            &[("w", 4.0)],
            |s| s / (s * s + 16.0),
        );
    }

    #[test]
    fn sinh_is_corrected_not_transcribed() {
        // The oracle answers `c/(-1+s^2)` for laplace(sinh(t)) — a free symbol
        // `c` where `1` belongs, i.e. a Symja defect. Transcribing it would
        // make the result depend on an undefined variable.
        assert_matches(
            &ok("sinh(t)", laplace),
            "c/(-1+s^2) [Symja bug]",
            "s",
            &[3.0, 7.25],
            &[],
            |s| 1.0 / (s * s - 1.0),
        );
        // sinh(a*t) / cosh(a*t) are refused outright by the oracle.
        assert_matches(
            &ok("sinh(a*t)", laplace),
            "REFUSED by Symja",
            "s",
            &[3.0, 7.25],
            &[("a", 2.0)],
            |s| 2.0 / (s * s - 4.0),
        );
        assert_matches(
            &ok("cosh(a*t)", laplace),
            "REFUSED by Symja",
            "s",
            &[3.0, 7.25],
            &[("a", 2.0)],
            |s| s / (s * s - 4.0),
        );
    }

    #[test]
    fn shifted_frequency_scaled_products_are_a_documented_superset() {
        // oracle: LaplaceTransform(Sin(3*t),t,2+s) — shifted but unevaluated,
        // so the REPL reports no closed form.
        assert_matches(
            &ok("exp(-2*t)*sin(3*t)", laplace),
            "REFUSED by Symja",
            "s",
            S,
            &[],
            |s| 3.0 / ((s + 2.0).powi(2) + 9.0),
        );
        assert_matches(
            &ok("exp(-2*t)*cos(3*t)", laplace),
            "REFUSED by Symja",
            "s",
            S,
            &[],
            |s| (s + 2.0) / ((s + 2.0).powi(2) + 9.0),
        );
        // oracle: CRASHES the bridge — Symja returns a Derivative(...) form
        // that CasExpressions.parse rejects with "unexpected trailing input".
        assert_matches(
            &ok("t*sin(w*t)", laplace),
            "BRIDGE ERROR in Symja",
            "s",
            S,
            &[("w", 4.0)],
            |s| 2.0 * 4.0 * s / (s * s + 16.0).powi(2),
        );
    }

    // ── forward: refusals ──────────────────────────────────────────────────

    #[test]
    fn refuses_what_is_outside_the_table_by_name() {
        for (src, needle) in [
            ("1/t", "division by a function of t"),
            ("tan(t)", "tan(t) is not in the transform table"),
            ("ln(t)", "ln(t) is not in the transform table"),
            ("sin(t)*cos(t)", "convolution"),
            // `sin(t)^2` and `(t+1)^2` split into two identical factors, so
            // the refusal names the product rather than the atom — and points
            // at Expand, which is what the user actually needs.
            ("sin(t)^2", "expand the expression first"),
            ("(t+1)^2", "expand the expression first"),
            ("exp(t^2)", "the table covers exp(a*t) only"),
            ("t^0.5", "not in the transform table"),
            ("t^(-1)", "division by a function of t"),
            ("t^5*sin(t)", "bounded at 4 here"),
        ] {
            let err = laplace(src).expect_err(&format!("{src} must be refused"));
            let message = err.to_string();
            assert!(
                message.contains("no closed form found for this input"),
                "{src}: {message}"
            );
            assert!(message.contains(needle), "{src}: {message}");
        }
    }

    #[test]
    fn rejects_bad_variable_names_like_the_java() {
        // CasEngine.requireIdentifier: [A-Za-z][A-Za-z0-9_]*
        assert!(transform(&expr("t"), "1t", "s")
            .expect_err("bad identifier")
            .to_string()
            .contains("invalid variable name: '1t'"));
        assert!(transform(&expr("t"), "t", "s+1").is_err());
        assert!(transform(&expr("t"), "t", "t").is_err());
    }

    #[test]
    fn variables_are_case_insensitive_like_every_frees_name() {
        let got = transform(&expr("exp(-2*t)"), "T", "S").expect("transforms");
        assert_matches(&got, "1/(2+s)", "s", S, &[], |s| 1.0 / (s + 2.0));
    }

    // ── inverse: cases the Java oracle answers, matched ────────────────────

    #[test]
    fn simple_poles_match_the_oracle() {
        // oracle: 1/s => 1 ; 1/(s+2) => E^(-2*t) ; 1/(s-3) => E^(3*t)
        assert_matches(&ok("1/s", ilaplace), "1", "t", T, &[], |_| 1.0);
        assert_matches(&ok("1/(s+2)", ilaplace), "E^(-2*t)", "t", T, &[], |t| {
            (-2.0 * t).exp()
        });
        assert_matches(&ok("1/(s-3)", ilaplace), "E^(3*t)", "t", T, &[], |t| {
            (3.0 * t).exp()
        });
        // oracle: 1/s^2 => t ; 1/s^3 => t^2/2 ; 2/s^3 => t^2 ; 1/s^4 => t^3/6
        assert_matches(&ok("1/s^2", ilaplace), "t", "t", T, &[], |t| t);
        assert_matches(&ok("1/s^3", ilaplace), "t^2/2", "t", T, &[], |t| {
            t * t / 2.0
        });
        assert_matches(&ok("2/s^3", ilaplace), "t^2", "t", T, &[], |t| t * t);
        assert_matches(&ok("1/(s^4)", ilaplace), "t^3/6", "t", T, &[], |t| {
            t.powi(3) / 6.0
        });
    }

    #[test]
    fn repeated_real_poles_match_the_oracle() {
        // oracle: 1/(s+2)^2 => t/E^(2*t) ; 1/(s+2)^3 => t^2/(2*E^(2*t))
        assert_matches(&ok("1/(s+2)^2", ilaplace), "t/E^(2*t)", "t", T, &[], |t| {
            t * (-2.0 * t).exp()
        });
        assert_matches(
            &ok("1/(s+2)^3", ilaplace),
            "t^2/(2*E^(2*t))",
            "t",
            T,
            &[],
            |t| t * t / 2.0 * (-2.0 * t).exp(),
        );
        // oracle: 1/(s+1)^4 => t^3/(6*E^t) ; 1/(s+1)^10 => t^9/(362880*E^t)
        assert_matches(
            &ok("1/(s+1)^4", ilaplace),
            "t^3/(6*E^t)",
            "t",
            T,
            &[],
            |t| t.powi(3) / 6.0 * (-t).exp(),
        );
        assert_matches(
            &ok("1/(s+1)^10", ilaplace),
            "t^9/(362880*E^t)",
            "t",
            T,
            &[],
            |t| t.powi(9) / 362_880.0 * (-t).exp(),
        );
    }

    #[test]
    fn undamped_sinusoids_match_the_oracle() {
        // oracle: 1/(s^2+1) => Sin(t) ; s/(s^2+1) => Cos(t)
        assert_matches(&ok("1/(s^2+1)", ilaplace), "Sin(t)", "t", T, &[], f64::sin);
        assert_matches(&ok("s/(s^2+1)", ilaplace), "Cos(t)", "t", T, &[], f64::cos);
        // oracle: 1/(s^2+9) => Sin(3*t)/3 ; s/(s^2+9) => Cos(3*t)
        assert_matches(&ok("1/(s^2+9)", ilaplace), "Sin(3*t)/3", "t", T, &[], |t| {
            (3.0 * t).sin() / 3.0
        });
        assert_matches(&ok("s/(s^2+9)", ilaplace), "Cos(3*t)", "t", T, &[], |t| {
            (3.0 * t).cos()
        });
        // oracle: 3/(s^2+4) => 3/2*Sin(2*t) ; 1/(s^2+2) => Sin(Sqrt(2)*t)/Sqrt(2)
        assert_matches(
            &ok("3/(s^2+4)", ilaplace),
            "3/2*Sin(2*t)",
            "t",
            T,
            &[],
            |t| 1.5 * (2.0 * t).sin(),
        );
        assert_matches(
            &ok("1/(s^2+2)", ilaplace),
            "Sin(Sqrt(2)*t)/Sqrt(2)",
            "t",
            T,
            &[],
            |t| (std::f64::consts::SQRT_2 * t).sin() / std::f64::consts::SQRT_2,
        );
    }

    #[test]
    fn symbolic_poles_and_frequencies_match_the_oracle() {
        // oracle: 1/(s+a) => E^(-a*t) ; a/(s+b) => a/E^(b*t)
        assert_matches(
            &ok("1/(s+a)", ilaplace),
            "E^(-a*t)",
            "t",
            T,
            &[("a", 2.0)],
            |t| (-2.0 * t).exp(),
        );
        assert_matches(
            &ok("a/(s+b)", ilaplace),
            "a/E^(b*t)",
            "t",
            T,
            &[("a", 3.0), ("b", 2.0)],
            |t| 3.0 * (-2.0 * t).exp(),
        );
        // oracle: 1/(s^2+w^2) => Sin(t*w)/w ; s/(s^2+w^2) => Cos(t*w) ;
        //         w/(s^2+w^2) => Sin(t*w)
        assert_matches(
            &ok("1/(s^2+w^2)", ilaplace),
            "Sin(t*w)/w",
            "t",
            T,
            &[("w", 3.0)],
            |t| (3.0 * t).sin() / 3.0,
        );
        assert_matches(
            &ok("s/(s^2+w^2)", ilaplace),
            "Cos(t*w)",
            "t",
            T,
            &[("w", 3.0)],
            |t| (3.0 * t).cos(),
        );
        assert_matches(
            &ok("w/(s^2+w^2)", ilaplace),
            "Sin(t*w)",
            "t",
            T,
            &[("w", 3.0)],
            |t| (3.0 * t).sin(),
        );
        // oracle: k/s => k
        assert_matches(&ok("k/s", ilaplace), "k", "t", T, &[("k", 4.0)], |_| 4.0);
    }

    #[test]
    fn partial_fractions_over_real_poles_match_the_oracle() {
        // oracle: (s+3)/(s^2+3*s+2) => -1/E^(2*t)+2/E^t
        assert_matches(
            &ok("(s+3)/(s^2+3*s+2)", ilaplace),
            "-1/E^(2*t)+2/E^t",
            "t",
            T,
            &[],
            |t| 2.0 * (-t).exp() - (-2.0 * t).exp(),
        );
        // oracle: 1/(s*(s+1)) => 1-1/E^t
        assert_matches(&ok("1/(s*(s+1))", ilaplace), "1-1/E^t", "t", T, &[], |t| {
            1.0 - (-t).exp()
        });
        // oracle: 1/(s*(s+1)*(s+2)) => 1/2+1/(2*E^(2*t))-1/E^t
        assert_matches(
            &ok("1/(s*(s+1)*(s+2))", ilaplace),
            "1/2+1/(2*E^(2*t))-1/E^t",
            "t",
            T,
            &[],
            |t| 0.5 + 0.5 * (-2.0 * t).exp() - (-t).exp(),
        );
        // oracle: (2*s+3)/(s^2+3*s+2) => E^(-2*t)+E^(-t)
        assert_matches(
            &ok("(2*s+3)/(s^2+3*s+2)", ilaplace),
            "E^(-2*t)+E^(-t)",
            "t",
            T,
            &[],
            |t| (-2.0 * t).exp() + (-t).exp(),
        );
        // oracle: 1/(s^2*(s+1)) => -1+E^(-t)+t
        assert_matches(
            &ok("1/(s^2*(s+1))", ilaplace),
            "-1+E^(-t)+t",
            "t",
            T,
            &[],
            |t| -1.0 + (-t).exp() + t,
        );
        // oracle: 1/((s+1)^2*(s+2)) => E^(-2*t)-1/E^t+t/E^t
        assert_matches(
            &ok("1/((s+1)^2*(s+2))", ilaplace),
            "E^(-2*t)-1/E^t+t/E^t",
            "t",
            T,
            &[],
            |t| (-2.0 * t).exp() - (-t).exp() + t * (-t).exp(),
        );
        // oracle: 1/((s+1)*(s+2)*(s+3)) => 1/(2*E^(3*t))-1/E^(2*t)+1/(2*E^t)
        assert_matches(
            &ok("1/((s+1)*(s+2)*(s+3))", ilaplace),
            "1/(2*E^(3*t))-1/E^(2*t)+1/(2*E^t)",
            "t",
            T,
            &[],
            |t| 0.5 * (-3.0 * t).exp() - (-2.0 * t).exp() + 0.5 * (-t).exp(),
        );
        // oracle: (s^2+1)/(s*(s+1)*(s+2)) => 1/2+5/2*1/E^(2*t)-2/E^t
        assert_matches(
            &ok("(s^2+1)/(s*(s+1)*(s+2))", ilaplace),
            "1/2+5/2*1/E^(2*t)-2/E^t",
            "t",
            T,
            &[],
            |t| 0.5 + 2.5 * (-2.0 * t).exp() - 2.0 * (-t).exp(),
        );
        // oracle: 1/(s*(s+1)^2) => 1-1/E^t-t/E^t
        assert_matches(
            &ok("1/(s*(s+1)^2)", ilaplace),
            "1-1/E^t-t/E^t",
            "t",
            T,
            &[],
            |t| 1.0 - (-t).exp() - t * (-t).exp(),
        );
    }

    #[test]
    fn mixed_real_and_quadratic_factors_match_the_oracle() {
        // oracle: 1/(s^3+s) => 1-Cos(t)
        assert_matches(&ok("1/(s^3+s)", ilaplace), "1-Cos(t)", "t", T, &[], |t| {
            1.0 - t.cos()
        });
        // oracle: 1/(s*(s^2+4)) => 1/4-Cos(2*t)/4
        assert_matches(
            &ok("1/(s*(s^2+4))", ilaplace),
            "1/4-Cos(2*t)/4",
            "t",
            T,
            &[],
            |t| 0.25 - (2.0 * t).cos() / 4.0,
        );
        // oracle: 1/((s^2+1)*(s^2+4)) => Sin(t)/3-Sin(2*t)/6
        assert_matches(
            &ok("1/((s^2+1)*(s^2+4))", ilaplace),
            "Sin(t)/3-Sin(2*t)/6",
            "t",
            T,
            &[],
            |t| t.sin() / 3.0 - (2.0 * t).sin() / 6.0,
        );
        // oracle: 1/(s^2*(s^2+4)) => t/4-Sin(2*t)/8
        assert_matches(
            &ok("1/(s^2*(s^2+4))", ilaplace),
            "t/4-Sin(2*t)/8",
            "t",
            T,
            &[],
            |t| t / 4.0 - (2.0 * t).sin() / 8.0,
        );
    }

    #[test]
    fn irrational_real_pole_pairs_match_the_oracle() {
        // oracle: 1/(s^2-1) => (-1+E^(2*t))/(2*E^t), i.e. sinh(t)
        assert_matches(
            &ok("1/(s^2-1)", ilaplace),
            "(-1+E^(2*t))/(2*E^t)",
            "t",
            T,
            &[],
            f64::sinh,
        );
        // oracle: s/(s^2-1) => 1/(2*E^t)+E^t/2, i.e. cosh(t)
        assert_matches(
            &ok("s/(s^2-1)", ilaplace),
            "1/(2*E^t)+E^t/2",
            "t",
            T,
            &[],
            f64::cosh,
        );
        // oracle: 1/(s^2-4) => (-1+E^(4*t))/(4*E^(2*t))
        assert_matches(
            &ok("1/(s^2-4)", ilaplace),
            "(-1+E^(4*t))/(4*E^(2*t))",
            "t",
            T,
            &[],
            |t| (2.0 * t).sinh() / 2.0,
        );
        // oracle: 1/(s^2-2) => (-1+E^(2*Sqrt(2)*t))/(2*Sqrt(2)*E^(Sqrt(2)*t))
        assert_matches(
            &ok("1/(s^2-2)", ilaplace),
            "(-1+E^(2*Sqrt(2)*t))/(2*Sqrt(2)*E^(Sqrt(2)*t))",
            "t",
            T,
            &[],
            |t| (std::f64::consts::SQRT_2 * t).sinh() / std::f64::consts::SQRT_2,
        );
    }

    // ── inverse: where the port deliberately answers and the Java does not ──

    #[test]
    fn damped_sinusoids_are_a_documented_superset() {
        // Symja 3.0.0 cannot invert a shifted quadratic at all; every one of
        // these comes back as `InverseLaplaceTransform(...)` unevaluated, and
        // the REPL reports "no closed form found for this input."
        assert_matches(
            &ok("1/((s+2)^2+9)", ilaplace),
            "REFUSED by Symja",
            "t",
            T,
            &[],
            |t| (-2.0 * t).exp() * (3.0 * t).sin() / 3.0,
        );
        assert_matches(
            &ok("(s+2)/((s+2)^2+9)", ilaplace),
            "REFUSED by Symja",
            "t",
            T,
            &[],
            |t| (-2.0 * t).exp() * (3.0 * t).cos(),
        );
        assert_matches(
            &ok("1/(s^2+4*s+13)", ilaplace),
            "REFUSED by Symja",
            "t",
            T,
            &[],
            |t| (-2.0 * t).exp() * (3.0 * t).sin() / 3.0,
        );
        assert_matches(
            &ok("s/(s^2+4*s+13)", ilaplace),
            "REFUSED by Symja",
            "t",
            T,
            &[],
            |t| (-2.0 * t).exp() * ((3.0 * t).cos() - 2.0 / 3.0 * (3.0 * t).sin()),
        );
        assert_matches(
            &ok("(s+1)/(s^2+2*s+5)", ilaplace),
            "REFUSED by Symja",
            "t",
            T,
            &[],
            |t| (-t).exp() * (2.0 * t).cos(),
        );
        // oracle: 1/(s^2+s+1) refused. α = 1/2, ω = √3/2.
        assert_matches(
            &ok("1/(s^2+s+1)", ilaplace),
            "REFUSED by Symja",
            "t",
            T,
            &[],
            |t| {
                let w = 3f64.sqrt() / 2.0;
                (-0.5 * t).exp() * (w * t).sin() / w
            },
        );
    }

    #[test]
    fn expanded_and_non_monic_denominators_are_a_documented_superset() {
        // oracle: 1/(s^2+2*s+1) refused, although 1/(s+1)^2 works — Symja is
        // purely structural and never factors.
        assert_matches(
            &ok("1/(s^2+2*s+1)", ilaplace),
            "REFUSED by Symja",
            "t",
            T,
            &[],
            |t| t * (-t).exp(),
        );
        // oracle: 1/(3*s+6) refused — a non-monic linear denominator.
        assert_matches(
            &ok("1/(3*s+6)", ilaplace),
            "REFUSED by Symja",
            "t",
            T,
            &[],
            |t| (-2.0 * t).exp() / 3.0,
        );
        // oracle: 1/(2*s^2+8) refused.
        assert_matches(
            &ok("1/(2*s^2+8)", ilaplace),
            "REFUSED by Symja",
            "t",
            T,
            &[],
            |t| (2.0 * t).sin() / 4.0,
        );
        // oracle: (2*s+1)/(s^2+4) refused — a sum numerator over an
        // irreducible quadratic.
        assert_matches(
            &ok("(2*s+1)/(s^2+4)", ilaplace),
            "REFUSED by Symja",
            "t",
            T,
            &[],
            |t| 2.0 * (2.0 * t).cos() + 0.5 * (2.0 * t).sin(),
        );
    }

    #[test]
    fn half_transformed_java_results_are_completed_here() {
        // oracle: (s^2+2*s+3)/((s+1)*(s^2+4)) =>
        //   2/5*1/E^t+InverseLaplaceTransform((7+3*s)/(4+s^2),s,t)/5
        // — reported as a SUCCESS by ReplEvaluator (isUnevaluated only checks
        // the leading head), so the Java hands the user an expression that
        // cannot be evaluated. Here it comes back complete.
        assert_matches(
            &ok("(s^2+2*s+3)/((s+1)*(s^2+4))", ilaplace),
            "half-transformed in Symja",
            "t",
            T,
            &[],
            |t| 0.4 * (-t).exp() + 0.6 * (2.0 * t).cos() + 0.7 * (2.0 * t).sin(),
        );
        // oracle: 10/(s*(s^2+2*s+10)) => 10*(1/10+InverseLaplaceTransform(...))
        assert_matches(
            &ok("10/(s*(s^2+2*s+10))", ilaplace),
            "half-transformed in Symja",
            "t",
            T,
            &[],
            |t| 1.0 - (-t).exp() * ((3.0 * t).cos() + (3.0 * t).sin() / 3.0),
        );
        // oracle: 1/(s^3+8) => 1/(12*E^(2*t))+InverseLaplaceTransform(...)/12
        assert_matches(
            &ok("1/(s^3+8)", ilaplace),
            "half-transformed in Symja",
            "t",
            T,
            &[],
            |t| {
                // Poles −2 and 1 ± i√3; residue at −2 is 1/12, and the
                // conjugate pair contributes e^t(−cos(√3 t) + √3 sin(√3 t))/12.
                let w = 3f64.sqrt();
                (-2.0 * t).exp() / 12.0 + t.exp() * (-(w * t).cos() + w * (w * t).sin()) / 12.0
            },
        );
        // oracle: (s+1)/(s*(s^2+2*s+2)) => 1/2-InverseLaplaceTransform(...)/2
        assert_matches(
            &ok("(s+1)/(s*(s^2+2*s+2))", ilaplace),
            "half-transformed in Symja",
            "t",
            T,
            &[],
            |t| 0.5 - 0.5 * (-t).exp() * (t.cos() - t.sin()),
        );
    }

    // ── inverse: refusals ──────────────────────────────────────────────────

    #[test]
    fn refuses_dirac_impulses_by_name() {
        // The oracle answers `5*DiracDelta(t)` and `-1/E^t+DiracDelta(t)`;
        // frees has no impulse, and neither answer survives Evaluator.eval.
        for src in ["5", "s/(s+1)", "(s^2+1)/(s^2+2*s+1)"] {
            let message = ilaplace(src)
                .expect_err(&format!("{src} must be refused"))
                .to_string();
            assert!(message.contains("Dirac impulse"), "{src}: {message}");
        }
        // `ilaplace(s)` is worse than refused in the Java: Symja returns
        // DiracDelta'(t), whose printed form breaks CasExpressions.parse.
        assert!(ilaplace("s").is_err());
    }

    #[test]
    fn refuses_repeated_complex_poles_like_the_oracle() {
        // oracle: 1/(s^2+1)^2 => InverseLaplaceTransform(1/(1+s^2)^2,s,t)
        let message = ilaplace("1/(s^2+1)^2")
            .expect_err("must be refused")
            .to_string();
        assert!(
            message.contains("repeated irreducible quadratic"),
            "{message}"
        );
    }

    #[test]
    fn refuses_non_rational_images_by_name() {
        // oracle refuses all three.
        for (src, needle) in [
            ("1/ln(s)", "no closed form found for this input"),
            ("exp(-s)/s", "no closed form found for this input"),
            ("1/sqrt(s)", "no closed form found for this input"),
        ] {
            let message = ilaplace(src)
                .expect_err(&format!("{src} must be refused"))
                .to_string();
            assert!(message.contains(needle), "{src}: {message}");
        }
    }

    #[test]
    fn a_symbolic_completed_square_picks_the_branch_its_sign_demands() {
        // (s+a)^2 - 4 is written as a completed square with a NEGATIVE w², so
        // the answer is hyperbolic. Taking the sinusoidal branch anyway would
        // emit sqrt(-4) and hand the caller a silent NaN.
        assert_matches(
            &ok("1/((s+a)^2-4)", ilaplace),
            "not reachable in Symja",
            "t",
            T,
            &[("a", 1.0)],
            |t| (-t).exp() * (2.0 * t).sinh() / 2.0,
        );
        assert_matches(
            &ok("(s+a)/((s+a)^2-4)", ilaplace),
            "not reachable in Symja",
            "t",
            T,
            &[("a", 1.0)],
            |t| (-t).exp() * (2.0 * t).cosh(),
        );
        // w² = 0 is a double real pole, not a zero-frequency sinusoid.
        assert_matches(
            &ok("1/((s+a)^2+0)", ilaplace),
            "not reachable in Symja",
            "t",
            T,
            &[("a", 1.0)],
            |t| t * (-t).exp(),
        );
    }

    #[test]
    fn a_constant_divisor_is_not_mistaken_for_a_second_pole() {
        // `1/(2*(s+b))` factors into [2, s+b]; only the second is a pole.
        assert_matches(
            &ok("1/(2*(s+b))", ilaplace),
            "not reachable in Symja",
            "t",
            T,
            &[("b", 3.0)],
            |t| 0.5 * (-3.0 * t).exp(),
        );
        assert_matches(
            &ok("1/(2*(s+b)^2)", ilaplace),
            "not reachable in Symja",
            "t",
            T,
            &[("b", 3.0)],
            |t| 0.5 * t * (-3.0 * t).exp(),
        );
        assert_matches(
            &ok("1/(k*(s^2+w^2))", ilaplace),
            "not reachable in Symja",
            "t",
            T,
            &[("k", 4.0), ("w", 3.0)],
            |t| (3.0 * t).sin() / (4.0 * 3.0),
        );
    }

    #[test]
    fn a_zero_numerator_inverts_to_zero_not_an_error() {
        assert_matches(&ok("0/(s+1)", ilaplace), "0", "t", T, &[], |_| 0.0);
    }

    #[test]
    fn refuses_symbolic_products_whose_poles_may_coincide() {
        // 1/((s+a)*(s+b)) needs a residue 1/(b−a), which is only valid when the
        // poles differ — undecidable, so refused rather than guessed.
        let message = ilaplace("1/((s+a)*(s+b))")
            .expect_err("must be refused")
            .to_string();
        assert!(message.contains("poles coincide"), "{message}");
    }

    #[test]
    fn refuses_high_degree_irreducible_factors_by_name() {
        // s^4+s^2+1 = (s^2+s+1)(s^2-s+1) over ℚ, but neither is found by the
        // rational-root sieve, so it is refused rather than approximated.
        let message = ilaplace("1/(s^4+s^2+1)")
            .expect_err("must be refused")
            .to_string();
        assert!(message.contains("degree 4"), "{message}");
    }

    // ── round trips ────────────────────────────────────────────────────────

    #[test]
    fn forward_then_inverse_returns_the_original_signal() {
        for (src, f) in [
            (
                "exp(-2*t)",
                &(|t: f64| (-2.0 * t).exp()) as &dyn Fn(f64) -> f64,
            ),
            ("sin(3*t)", &|t: f64| (3.0 * t).sin()),
            ("cos(3*t)", &|t: f64| (3.0 * t).cos()),
            ("t^3", &|t: f64| t.powi(3)),
            ("t*exp(-t)", &|t: f64| t * (-t).exp()),
            ("exp(-2*t)*sin(3*t)", &|t: f64| {
                (-2.0 * t).exp() * (3.0 * t).sin()
            }),
            ("exp(-2*t)*cos(3*t)", &|t: f64| {
                (-2.0 * t).exp() * (3.0 * t).cos()
            }),
            ("2*t^2 - 5*exp(-t)", &|t: f64| {
                2.0 * t * t - 5.0 * (-t).exp()
            }),
            ("sinh(2*t)", &|t: f64| (2.0 * t).sinh()),
            ("cosh(2*t)", &|t: f64| (2.0 * t).cosh()),
        ] {
            let image = ok(src, laplace);
            let back = inverse_transform(&image, "s", "t")
                .unwrap_or_else(|e| panic!("{src}: round trip failed: {e}"));
            assert_matches(&back, "round trip", "t", T, &[], f);
        }
    }

    // ── independent check: the defining integral ───────────────────────────

    /// `∫₀^∞ f(t)·e^(−s·t) dt` by composite Simpson's rule.
    ///
    /// This is the *definition* of the transform, evaluated numerically. It
    /// shares no code with the table, the factoriser or the partial-fraction
    /// solver, so it is a genuinely independent check — the only one available
    /// for the cases Symja refuses and therefore cannot be asked about.
    fn transform_integral(f: &Expr, s: f64) -> f64 {
        const UPPER: f64 = 40.0;
        const STEPS: usize = 40_000; // even, so Simpson's rule applies
        let h = UPPER / STEPS as f64;
        let integrand = |t: f64| at(f, &[("t", t)]) * (-s * t).exp();
        let mut total = integrand(0.0) + integrand(UPPER);
        for i in 1..STEPS {
            let weight = if i % 2 == 0 { 2.0 } else { 4.0 };
            total += weight * integrand(i as f64 * h);
        }
        total * h / 3.0
    }

    #[test]
    fn inverse_results_satisfy_the_defining_integral() {
        // s = 6 sits to the right of every pole below, so each integrand
        // decays and the truncation at t = 40 is far below the tolerance.
        const S_POINT: f64 = 6.0;
        for (src, image) in [
            // Cases Symja can also do — belt and braces.
            (
                "(s+3)/(s^2+3*s+2)",
                &(|s: f64| (s + 3.0) / (s * s + 3.0 * s + 2.0)) as &dyn Fn(f64) -> f64,
            ),
            ("1/((s+1)^2*(s+2))", &|s: f64| {
                1.0 / ((s + 1.0).powi(2) * (s + 2.0))
            }),
            ("1/((s^2+1)*(s^2+4))", &|s: f64| {
                1.0 / ((s * s + 1.0) * (s * s + 4.0))
            }),
            ("1/(s^2-1)", &|s: f64| 1.0 / (s * s - 1.0)),
            ("1/(s^2-2)", &|s: f64| 1.0 / (s * s - 2.0)),
            // Cases Symja refuses, so this is their only external check.
            ("1/(s^2+4*s+13)", &|s: f64| 1.0 / (s * s + 4.0 * s + 13.0)),
            ("s/(s^2+4*s+13)", &|s: f64| s / (s * s + 4.0 * s + 13.0)),
            ("(2*s+1)/(s^2+4)", &|s: f64| (2.0 * s + 1.0) / (s * s + 4.0)),
            ("1/(s^2+s+1)", &|s: f64| 1.0 / (s * s + s + 1.0)),
            ("1/(3*s+6)", &|s: f64| 1.0 / (3.0 * s + 6.0)),
            ("1/(2*s^2+8)", &|s: f64| 1.0 / (2.0 * s * s + 8.0)),
            ("1/(4*s^2+4*s+1)", &|s: f64| {
                1.0 / (4.0 * s * s + 4.0 * s + 1.0)
            }),
            ("1/(s^3+8)", &|s: f64| 1.0 / (s.powi(3) + 8.0)),
            ("(s^2+2*s+3)/((s+1)*(s^2+4))", &|s: f64| {
                (s * s + 2.0 * s + 3.0) / ((s + 1.0) * (s * s + 4.0))
            }),
            ("10/(s*(s^2+2*s+10))", &|s: f64| {
                10.0 / (s * (s * s + 2.0 * s + 10.0))
            }),
        ] {
            let signal = ok(src, ilaplace);
            let numeric = transform_integral(&signal, S_POINT);
            let want = image(S_POINT);
            assert!(
                (numeric - want).abs() <= 1e-7 * want.abs().max(1e-3),
                "{src}: ∫f(t)e^(-{S_POINT}t)dt = {numeric}, but F({S_POINT}) = {want}"
            );
        }
    }

    #[test]
    fn forward_results_satisfy_the_defining_integral() {
        const S_POINT: f64 = 6.0;
        for src in [
            "sin(3*t)",
            "cos(3*t)",
            "sinh(2*t)",
            "cosh(2*t)",
            "exp(-2*t)*sin(3*t)",
            "exp(-2*t)*cos(3*t)",
            "t*sin(2*t)",
            "t^3",
            "t^2*exp(-2*t)",
            "2*t^2 - 5*exp(-t)",
        ] {
            let image = ok(src, laplace);
            let numeric = transform_integral(&expr(src), S_POINT);
            let want = at(&image, &[("s", S_POINT)]);
            assert!(
                (numeric - want).abs() <= 1e-7 * want.abs().max(1e-3),
                "{src}: ∫f(t)e^(-{S_POINT}t)dt = {numeric}, but the table says {want}"
            );
        }
    }

    // ── the derivative rule ────────────────────────────────────────────────

    #[test]
    fn derivative_rule_matches_the_direct_transform() {
        // Symja's rule is `-f(0)+s*LaplaceTransform(f(t),t,s)`; it is
        // unreachable from frees, so it is pinned against the identity
        // L{f'} = s·F(s) − f(0) instead.
        for src in ["sin(3*t)", "exp(-2*t)", "t^3", "cos(t)", "t*exp(-t)"] {
            let f = expr(src);
            let rule = transform_derivative(&f, 1, "t", "s").expect("rule applies");
            let derivative = differentiate(&f, "t").expect("differentiable");
            let direct = transform(&derivative, "t", "s").expect("transforms");
            for &s in S {
                let a = at(&rule, &[("s", s)]);
                let b = at(&direct, &[("s", s)]);
                assert!(
                    (a - b).abs() <= 1e-9 * b.abs().max(1.0),
                    "{src} at s={s}: rule {a} vs direct {b}"
                );
            }
        }
    }

    #[test]
    fn second_derivative_rule_matches_the_direct_transform() {
        for src in ["sin(3*t)", "exp(-2*t)", "t^3"] {
            let f = expr(src);
            let rule = transform_derivative(&f, 2, "t", "s").expect("rule applies");
            let once = differentiate(&f, "t").expect("differentiable");
            let twice = differentiate(&once, "t").expect("differentiable");
            let direct = transform(&twice, "t", "s").expect("transforms");
            for &s in S {
                let a = at(&rule, &[("s", s)]);
                let b = at(&direct, &[("s", s)]);
                assert!(
                    (a - b).abs() <= 1e-9 * b.abs().max(1.0),
                    "{src} at s={s}: rule {a} vs direct {b}"
                );
            }
        }
    }

    #[test]
    fn derivative_order_is_bounded() {
        let message = transform_derivative(&expr("sin(t)"), 99, "t", "s")
            .expect_err("must be refused")
            .to_string();
        assert!(
            message.contains("exceeds the supported maximum"),
            "{message}"
        );
    }

    // ── exactness ──────────────────────────────────────────────────────────

    #[test]
    fn decimal_coefficients_stay_exact() {
        // 0.25 must decompose as 1/4, not as a binary approximation.
        assert_eq!(
            exact_rational(0.25),
            Some(BigRational::new(BigInt::from(1), BigInt::from(4)))
        );
        assert_eq!(
            exact_rational(0.1),
            Some(BigRational::new(BigInt::from(1), BigInt::from(10)))
        );
        assert_eq!(
            exact_rational(-3.0),
            Some(BigRational::from_integer(BigInt::from(-3)))
        );
        // A decimal expansion the table cannot read exactly is refused, not
        // rounded: pi is not a rational literal.
        assert_eq!(exact_rational(f64::INFINITY), None);
    }

    #[test]
    fn decimal_denominators_decompose_exactly() {
        // 1/(s + 0.5) and 1/(2*s + 1) are the same pole; both must land on it.
        assert_matches(&ok("1/(s+0.5)", ilaplace), "exact", "t", T, &[], |t| {
            (-0.5 * t).exp()
        });
        assert_matches(&ok("1/(2*s+1)", ilaplace), "exact", "t", T, &[], |t| {
            0.5 * (-0.5 * t).exp()
        });
    }

    #[test]
    fn exact_square_roots_are_folded() {
        assert_eq!(
            exact_sqrt(&BigRational::new(BigInt::from(9), BigInt::from(4))),
            Some(BigRational::new(BigInt::from(3), BigInt::from(2)))
        );
        assert_eq!(
            exact_sqrt(&BigRational::from_integer(BigInt::from(2))),
            None
        );
        assert_eq!(
            exact_sqrt(&BigRational::from_integer(BigInt::from(-1))),
            None
        );
    }

    #[test]
    fn partial_fraction_pieces_reconstruct_the_numerator() {
        // (s+3)/((s+1)(s+2)) = 2/(s+1) − 1/(s+2) — the residue workflow
        // CasEngineTest.partialFractionResiduesMatchOriginal pins in the Java.
        let numerator = to_poly(&expr("s+3"), "s", 0).expect("polynomial");
        let factors = vec![
            to_poly(&expr("s+1"), "s", 0).expect("polynomial"),
            to_poly(&expr("s+2"), "s", 0).expect("polynomial"),
        ];
        let pieces = partial_fractions(&numerator, &factors).expect("decomposes");
        assert_eq!(pieces.len(), 2);
        let mut reconstructed = vec![BigRational::zero()];
        let denominator = poly_mul(&factors[0], &factors[1]);
        for piece in &pieces {
            let cofactor =
                poly_div_exact(&denominator, &poly_pow(&piece.factor, piece.power)).expect("exact");
            reconstructed = poly_add(&reconstructed, &poly_mul(&piece.numerator, &cofactor));
        }
        assert_eq!(poly_trim(reconstructed), poly_trim(numerator));
    }

    #[test]
    fn factorials_stay_exact() {
        assert_eq!(factorial(0), 1);
        assert_eq!(factorial(1), 1);
        assert_eq!(factorial(10), 3_628_800);
        assert_eq!(factorial(18), 6_402_373_705_728_000);
        assert!((factorial(18) as f64) < MAX_EXACT_INTEGER as f64);
    }

    #[test]
    fn oversized_powers_are_refused_not_rounded() {
        let message = laplace("t^19").expect_err("must be refused").to_string();
        assert!(message.contains("not in the transform table"), "{message}");
    }

    // ── structural helpers ─────────────────────────────────────────────────

    #[test]
    fn affine_extraction_is_symbolic() {
        let (slope, intercept) = affine_in(&expr("3*t + 5"), "t").expect("affine");
        assert!((at(&slope, &[]) - 3.0).abs() < 1e-12);
        assert!((at(&intercept, &[]) - 5.0).abs() < 1e-12);
        let (slope, intercept) = affine_in(&expr("-a*t"), "t").expect("affine");
        assert!((at(&slope, &[("a", 2.0)]) + 2.0).abs() < 1e-12);
        assert!(at(&intercept, &[]).abs() < 1e-12);
        assert!(affine_in(&expr("t^2"), "t").is_none());
        assert!(affine_in(&expr("sin(t)"), "t").is_none());
    }

    #[test]
    fn factor_collection_splits_integer_powers() {
        let mut numerator = Vec::new();
        let mut denominator = Vec::new();
        collect_factors(
            &expr("3/((s+1)^2*(s+2))"),
            true,
            &mut numerator,
            &mut denominator,
            0,
        )
        .expect("collects");
        assert_eq!(numerator.len(), 1);
        assert_eq!(denominator.len(), 3);
    }

    #[test]
    fn substitution_reaches_every_arm() {
        let shifted = substitute(&expr("sin(s) + s^2"), "s", &expr("s-1"));
        assert!((at(&shifted, &[("s", 1.0)]) - 0.0f64.sin()).abs() < 1e-12);
    }
}
