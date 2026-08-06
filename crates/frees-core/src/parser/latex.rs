//! AST → LaTeX for the Formatted Equations window.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/parser/LatexConverter.java`
//! (271 LOC). The expected strings in the tests below are copied verbatim from
//! `LatexConverterTest.java` — they define the output exactly.
//!
//! ## Public surface (Phase-4 contract)
//!
//! * [`equation_to_latex`] — renders `lhs = rhs`.
//! * [`expr_to_latex`] — renders a single expression.
//! * [`residue_to_latex`] / [`ResidueResult`] — the Java `toLatex(ResidueResult)`
//!   overload (partial-fraction display for the control-systems tools).
//!   [`ResidueResult`] mirrors `cas/PolynomialHelpers.ResidueResult` and should
//!   move to the CAS module when that ports.
//!
//! The display-name-aware forms ([`equation_to_latex_named`],
//! [`expr_to_latex_named`]) are `pub(crate)`: the Java entry points take a
//! `Map<String, String>` of lowercase name → display spelling (identifiers are
//! stored lowercase; `Solution.display_names` carries the user's spellings).
//! The two contract functions render with an empty map; the formatted-equations
//! wiring passes the real map when it lands.
//!
//! ## Divergences from Java (deliberate — this is a display path)
//!
//! The converter is a **total function**: it never panics where Java throws.
//!
//! * Operators outside `+ - * / ^` throw `IllegalStateException` in Java
//!   (they reach it as the private-use element-wise chars `⊙ ⊘ ∖ ↑`). Here
//!   they render those same engine glyphs: `\odot`, `\oslash`, `\setminus`,
//!   `\uparrow`, and `\backslash` for left division.
//! * A known function with too few arguments (Java: `IndexOutOfBoundsException`)
//!   falls back to the generic `\text{name}\left(args\right)` form.
//! * A malformed `prop$` name (Java: `StringIndexOutOfBoundsException`)
//!   renders with whatever `$`-parts it has.
//! * The `_dot`/`_hat` suffix check is ASCII-case-insensitive rather than
//!   full-Unicode `toLowerCase()` — identical for every name the lexer admits.

use std::collections::HashMap;

use crate::ast::{BinOp, Equation, Expr};
use crate::eval::Scope;

const RIGHT_PAREN: &str = "\\right)";
const TEXT_OPEN: &str = "\\text{";
const CLOSE_LEFT_PAREN: &str = "}\\left(";

/// Render an equation as `lhs = rhs`.
///
/// Port of `LatexConverter.toLatex(Equation, Map)` with an empty display map.
pub fn equation_to_latex(eq: &Equation) -> String {
    equation_to_latex_named(eq, &HashMap::new())
}

/// Render a single expression.
///
/// Port of `LatexConverter.toLatex(Expr, Map)` with an empty display map.
pub fn expr_to_latex(e: &Expr) -> String {
    expr_to_latex_named(e, &HashMap::new())
}

/// [`equation_to_latex`] with display spellings (lowercase name → spelling).
pub(crate) fn equation_to_latex_named(
    eq: &Equation,
    display_names: &HashMap<String, String>,
) -> String {
    format!(
        "{} = {}",
        expr_to_latex_named(&eq.lhs, display_names),
        expr_to_latex_named(&eq.rhs, display_names)
    )
}

/// [`expr_to_latex`] with display spellings (lowercase name → spelling).
pub(crate) fn expr_to_latex_named(e: &Expr, display_names: &HashMap<String, String>) -> String {
    match e {
        Expr::Num {
            value,
            unit,
            is_imaginary,
        } => {
            let mut val = format_value(*value);
            if *is_imaginary {
                val = if *value == 1.0 {
                    "i".to_string()
                } else if *value == -1.0 {
                    "-i".to_string()
                } else {
                    format!("{val}i")
                };
            }
            match unit.as_deref() {
                // Java: `unit != null && !unit.isBlank()`.
                Some(u) if !u.trim().is_empty() => format!("{val}\\,\\left[{u}\\right]"),
                _ => val,
            }
        }

        Expr::Str(value) => format!("{TEXT_OPEN}'{value}'}}"),

        Expr::Var(name) => {
            let disp = display_names.get(name).map(String::as_str).unwrap_or(name);
            format_variable(disp)
        }

        Expr::Neg(operand) => {
            let op_str = expr_to_latex_named(operand, display_names);
            // Java parenthesizes only when the operand is a `+` or `-` BinOp.
            if matches!(
                operand.as_ref(),
                Expr::BinOp {
                    op: BinOp::Add | BinOp::Sub,
                    ..
                }
            ) {
                format!("-\\left({op_str}{RIGHT_PAREN}")
            } else {
                format!("-{op_str}")
            }
        }

        Expr::BinOp { op, left, right } => {
            let l = expr_to_latex_named(left, display_names);
            let r = expr_to_latex_named(right, display_names);
            match op {
                BinOp::Add => format!("{l} + {r}"),
                BinOp::Sub => format!("{l} - {r}"),
                // A numeric coefficient gets a thin space (`2\,y`); everything
                // else an explicit `\cdot`.
                BinOp::Mul => {
                    if matches!(left.as_ref(), Expr::Num { .. }) {
                        format!("{l}\\,{r}")
                    } else {
                        format!("{l}\\cdot {r}")
                    }
                }
                BinOp::Div => format!("\\frac{{{l}}}{{{r}}}"),
                BinOp::Pow => {
                    let base = if matches!(left.as_ref(), Expr::BinOp { .. } | Expr::Neg(_)) {
                        format!("\\left({l}{RIGHT_PAREN}")
                    } else {
                        l
                    };
                    format!("{base}^{{{r}}}")
                }
                // Java throws `IllegalStateException("Unknown operator")` for
                // these — render the engine's own glyphs instead (see module
                // docs).
                BinOp::LeftDiv => format!("{l}\\backslash {r}"),
                BinOp::ElemMul => format!("{l}\\odot {r}"),
                BinOp::ElemDiv => format!("{l}\\oslash {r}"),
                BinOp::ElemLeftDiv => format!("{l}\\setminus {r}"),
                BinOp::ElemPow => format!("{l}\\uparrow {r}"),
            }
        }

        Expr::Call { function, args } => {
            let arg_lates: Vec<String> = args
                .iter()
                .map(|a| expr_to_latex_named(a, display_names))
                .collect();
            let args_str = arg_lates.join(", ");
            if function.starts_with("prop$") {
                property_call_latex(function, &arg_lates)
            } else {
                call_latex(function, args, &arg_lates, &args_str, display_names)
            }
        }

        Expr::ArrayAccess { name, indices } => {
            let disp = display_names.get(name).map(String::as_str).unwrap_or(name);
            let base = format_variable(disp);
            let idx_lates: Vec<String> = indices
                .iter()
                .map(|a| expr_to_latex_named(a, display_names))
                .collect();
            format!("{base}_{{{}}}", idx_lates.join(", "))
        }

        Expr::Range { start, end } => format!(
            "{}\\dots{}",
            expr_to_latex_named(start, display_names),
            expr_to_latex_named(end, display_names)
        ),

        Expr::ArrayLiteral(elements) => {
            let elems: Vec<String> = elements
                .iter()
                .map(|a| expr_to_latex_named(a, display_names))
                .collect();
            format!("\\left[{}\\right]", elems.join(", "))
        }

        Expr::Compare { op, left, right } => format!(
            "{} {} {}",
            expr_to_latex_named(left, display_names),
            op.as_str(),
            expr_to_latex_named(right, display_names)
        ),

        Expr::Logical { op, left, right } => format!(
            "{} \\text{{ {} }} {}",
            expr_to_latex_named(left, display_names),
            op.as_str(),
            expr_to_latex_named(right, display_names)
        ),

        Expr::Not(operand) => format!("\\neg {}", expr_to_latex_named(operand, display_names)),
    }
}

/// The named-function switch of the Java `Expr.Call` arm.
///
/// Returns the generic `\text{name}\left(args\right)` form for unknown names —
/// and, unlike Java (which throws), also for known names missing arguments.
fn call_latex(
    function: &str,
    arg_exprs: &[Expr],
    args: &[String],
    args_str: &str,
    display_names: &HashMap<String, String>,
) -> String {
    let known = match function {
        "sqrt" => args.first().map(|a| format!("\\sqrt{{{a}}}")),
        "cbrt" => args.first().map(|a| format!("\\sqrt[3]{{{a}}}")),
        "sin" => Some(format!("\\sin\\left({args_str}{RIGHT_PAREN}")),
        "cos" => Some(format!("\\cos\\left({args_str}{RIGHT_PAREN}")),
        "tan" => Some(format!("\\tan\\left({args_str}{RIGHT_PAREN}")),
        "asin" | "arcsin" => Some(format!("\\arcsin\\left({args_str}{RIGHT_PAREN}")),
        "acos" | "arccos" => Some(format!("\\arccos\\left({args_str}{RIGHT_PAREN}")),
        "atan" | "arctan" => Some(format!("\\arctan\\left({args_str}{RIGHT_PAREN}")),
        "sinh" => Some(format!("\\sinh\\left({args_str}{RIGHT_PAREN}")),
        "cosh" => Some(format!("\\cosh\\left({args_str}{RIGHT_PAREN}")),
        "tanh" => Some(format!("\\tanh\\left({args_str}{RIGHT_PAREN}")),
        "arcsinh" => Some(format!(
            "{TEXT_OPEN}arcsinh{CLOSE_LEFT_PAREN}{args_str}{RIGHT_PAREN}"
        )),
        "arccosh" => Some(format!(
            "{TEXT_OPEN}arccosh{CLOSE_LEFT_PAREN}{args_str}{RIGHT_PAREN}"
        )),
        "arctanh" => Some(format!(
            "{TEXT_OPEN}arctanh{CLOSE_LEFT_PAREN}{args_str}{RIGHT_PAREN}"
        )),
        "ln" => Some(format!("\\ln\\left({args_str}{RIGHT_PAREN}")),
        "log10" => Some(format!("\\log_{{10}}\\left({args_str}{RIGHT_PAREN}")),
        "log2" => Some(format!("\\log_{{2}}\\left({args_str}{RIGHT_PAREN}")),
        "exp" => args.first().map(|a| format!("e^{{{a}}}")),
        "abs" => args.first().map(|a| format!("\\left|{a}\\right|")),
        "convert" => Some(format!(
            "{TEXT_OPEN}Convert{CLOSE_LEFT_PAREN}{args_str}{RIGHT_PAREN}"
        )),
        "besselj" | "bessel_j" => bessel_order('J', args),
        "besseli" | "bessel_i" => bessel_order('I', args),
        "bessely" | "bessel_y" => bessel_order('Y', args),
        "besselk" | "bessel_k" => bessel_order('K', args),
        "besselj0" | "bessel_j0" => bessel_fixed("J_0", args),
        "besselj1" | "bessel_j1" => bessel_fixed("J_1", args),
        "besseli0" | "bessel_i0" => bessel_fixed("I_0", args),
        "besseli1" | "bessel_i1" => bessel_fixed("I_1", args),
        "bessely0" | "bessel_y0" => bessel_fixed("Y_0", args),
        "bessely1" | "bessel_y1" => bessel_fixed("Y_1", args),
        "besselk0" | "bessel_k0" => bessel_fixed("K_0", args),
        "besselk1" | "bessel_k1" => bessel_fixed("K_1", args),
        "chi_square" => Some(format!("\\chi^2\\left({args_str}{RIGHT_PAREN}")),
        // Render `tf(num, den)` as the Laplace fraction num(s)/den(s); fall
        // back to the plain call form when the coefficients are not constant
        // array literals (the Java arm catches the RuntimeException).
        "tf" => expand_tf_call(arg_exprs, "s")
            .ok()
            .map(|expanded| expr_to_latex_named(&expanded, display_names)),
        _ => None,
    };
    known.unwrap_or_else(|| {
        format!("{TEXT_OPEN}{function}{CLOSE_LEFT_PAREN}{args_str}{RIGHT_PAREN}")
    })
}

/// `J_{n}\left(x\right)` from `besselj(x, n)` — order in args\[1\], argument in
/// args\[0\].
fn bessel_order(letter: char, args: &[String]) -> Option<String> {
    match args {
        [x, n, ..] => Some(format!("{letter}_{{{n}{CLOSE_LEFT_PAREN}{x}{RIGHT_PAREN}")),
        _ => None,
    }
}

/// `J_0\left(x\right)`-style fixed-order Bessel rendering.
fn bessel_fixed(prefix: &str, args: &[String]) -> Option<String> {
    args.first()
        .map(|x| format!("{prefix}\\left({x}{RIGHT_PAREN}"))
}

/// `Enthalpy(R134a, T=T_1, x=1)` from `prop$enthalpy$r134a$t$x` + rendered args.
///
/// Port of `LatexConverter.propertyCallLatex`.
fn property_call_latex(func: &str, arg_lates: &[String]) -> String {
    // Java `split("\\$")` drops trailing empty strings.
    let mut parts: Vec<&str> = func.split('$').collect();
    while parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    let output = capitalize_first(parts.get(1).copied().unwrap_or(""));
    // Chemistry calls (prop$molarmass, prop$heatingvalue, prop$stoichafr)
    // carry their fluid/formula/mode as string arguments rather than in the
    // encoded name, so render them straight from the args.
    if parts.len() < 3 {
        return format!(
            "{TEXT_OPEN}{output}{CLOSE_LEFT_PAREN}{}{RIGHT_PAREN}",
            arg_lates.join(", ")
        );
    }
    let mut sb = format!("{TEXT_OPEN}{output}}}\\left(\\mathrm{{{}}}", parts[2]);
    let mut i = 0;
    while i + 3 < parts.len() && i < arg_lates.len() {
        sb.push_str(", ");
        sb.push_str(parts[i + 3]);
        sb.push('=');
        sb.push_str(&arg_lates[i]);
        i += 1;
    }
    sb.push_str(RIGHT_PAREN);
    sb
}

/// First character uppercased, rest untouched — Java
/// `s.substring(0, 1).toUpperCase() + s.substring(1)`.
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// `T_in` → `T_{in}`, `x_dot` → `\dot{x}`, `x_1_hat` → `\hat{x_{1}}`.
///
/// Port of `LatexConverter.formatVariable`: strip one trailing `_dot`/`_hat`
/// decoration (case-insensitive), then everything after the **first** interior
/// underscore becomes one subscript. A leading underscore is not a subscript
/// separator (`firstUnderscore > 0` in Java).
fn format_variable(display_spelling: &str) -> String {
    let mut name = display_spelling;
    let mut has_dot = false;
    let mut has_hat = false;

    if ends_with_ignore_ascii_case(name, "_dot") {
        has_dot = true;
        name = &name[..name.len() - 4];
    } else if ends_with_ignore_ascii_case(name, "_hat") {
        has_hat = true;
        name = &name[..name.len() - 4];
    }

    let mut latex = match name.find('_') {
        Some(idx) if idx > 0 => format!("{}_{{{}}}", &name[..idx], &name[idx + 1..]),
        _ => name.to_string(),
    };

    if has_dot {
        latex = format!("\\dot{{{latex}}}");
    } else if has_hat {
        latex = format!("\\hat{{{latex}}}");
    }

    latex
}

/// ASCII-case-insensitive `endsWith`, safe on any UTF-8 (`suffix` must be
/// ASCII, which `_dot`/`_hat` are).
fn ends_with_ignore_ascii_case(name: &str, suffix: &str) -> bool {
    name.len() >= suffix.len()
        && name
            .get(name.len() - suffix.len()..)
            .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix))
}

/// Integral doubles print as integers (`5`, not `5.0`); everything else prints
/// exactly like Java `String.valueOf(double)`.
///
/// Port of `LatexConverter.formatValue`: `val == (long) val` → `%d` (the Rust
/// `as i64` cast has the same truncate-toward-zero / saturate / NaN→0
/// semantics as the JVM `(long)` conversion), else `Double.toString`.
fn format_value(val: f64) -> String {
    if val == val as i64 as f64 {
        format!("{}", val as i64)
    } else {
        java_double_to_string(val)
    }
}

/// Java `Double.toString(double)` (Java 19+ shortest-repr semantics, which
/// Rust's shortest-round-trip float formatting matches digit-for-digit):
/// plain decimal for `1e-3 <= |d| < 1e7`, otherwise computerized scientific
/// notation `D.DDDE±X`, always with at least one fractional digit; `NaN`,
/// `Infinity`, `-Infinity` spelled out.
fn java_double_to_string(val: f64) -> String {
    if val.is_nan() {
        return "NaN".to_string();
    }
    if val.is_infinite() {
        return if val > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if val == 0.0 {
        // Unreachable via `format_value` (0 is integral) but kept total.
        return if val.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_string();
    }
    let abs = val.abs();
    if (1e-3..1e7).contains(&abs) {
        let s = format!("{val}");
        if s.contains('.') {
            s
        } else {
            format!("{s}.0")
        }
    } else {
        let s = format!("{val:e}");
        let (mantissa, exponent) = s.split_once('e').unwrap_or((s.as_str(), "0"));
        if mantissa.contains('.') {
            format!("{mantissa}E{exponent}")
        } else {
            format!("{mantissa}.0E{exponent}")
        }
    }
}

// ---------------------------------------------------------------------------
// Partial-fraction (residue) rendering
// ---------------------------------------------------------------------------

/// A partial-fraction expansion: `Σ rᵢ / (s - pᵢ)^ordᵢ + k`.
///
/// Mirror of `cas/PolynomialHelpers.ResidueResult` (`double[][] residues`,
/// `double[][] poles`, `int[] orders`, `double k`) — defined here until the
/// CAS module ports; complex values are `[re, im]` pairs.
#[derive(Debug, Clone, PartialEq)]
pub struct ResidueResult {
    /// Residue of each pole as `[re, im]`.
    pub residues: Vec<[f64; 2]>,
    /// Pole locations as `[re, im]`.
    pub poles: Vec<[f64; 2]>,
    /// Multiplicity of each pole.
    pub orders: Vec<i32>,
    /// Direct (polynomial) term.
    pub k: f64,
}

/// Render a [`ResidueResult`] as a sum of `\frac{r}{s - p}` terms plus the
/// direct term.
///
/// Port of `LatexConverter.toLatex(ResidueResult)`. Zero residues are skipped;
/// an all-zero expansion renders the direct term alone (`"0"` when `k == 0`).
/// (The Java body also computes a `pLatex` string it never reads — omitted.)
pub fn residue_to_latex(res: &ResidueResult) -> String {
    let mut sb = String::new();
    let mut first = true;
    for ((r, p), &ord) in res.residues.iter().zip(&res.poles).zip(&res.orders) {
        let (r_re, r_im) = (r[0], r[1]);
        let (p_re, p_im) = (p[0], p[1]);

        if r_re == 0.0 && r_im == 0.0 {
            continue;
        }

        let r_latex = format_complex(r_re, r_im);

        if !first {
            sb.push_str(" + ");
        } else {
            first = false;
        }

        sb.push_str("\\frac{");
        sb.push_str(&r_latex);
        sb.push_str("}{");
        let denom_base = if p_re == 0.0 && p_im == 0.0 {
            "s".to_string()
        } else if p_re > 0.0 && p_im == 0.0 {
            format!("s - {}", format_value(p_re))
        } else if p_re < 0.0 && p_im == 0.0 {
            format!("s + {}", format_value(-p_re))
        } else {
            // Complex pole: `s - p` written as `s ± (re ∓ im·i)`.
            let sign = if p_re > 0.0 { "- " } else { "+ " };
            let im = if p_re > 0.0 { p_im } else { -p_im };
            format!(
                "s {sign}\\left({}{RIGHT_PAREN}",
                format_complex(p_re.abs(), im)
            )
        };
        if ord > 1 {
            sb.push_str(&format!("\\left({denom_base}{RIGHT_PAREN}^{{{ord}}}"));
        } else {
            sb.push_str(&denom_base);
        }
        sb.push('}');
    }

    if res.k != 0.0 || first {
        if !first {
            sb.push_str(if res.k > 0.0 { " + " } else { " - " });
            sb.push_str(&format_value(res.k.abs()));
        } else {
            sb.push_str(&format_value(res.k));
        }
    }
    sb
}

/// `2 + 3i` / `-i` / `2i` / `5` — port of `LatexConverter.formatComplex`.
fn format_complex(re: f64, im: f64) -> String {
    if im == 0.0 {
        return format_value(re);
    }
    if re == 0.0 {
        return if im == 1.0 {
            "i".to_string()
        } else if im == -1.0 {
            "-i".to_string()
        } else {
            format!("{}i", format_value(im))
        };
    }
    let sign = if im > 0.0 { " + " } else { " - " };
    let abs_im = im.abs();
    let im_part = if abs_im == 1.0 {
        "i".to_string()
    } else {
        format!("{}i", format_value(abs_im))
    };
    format!("{}{sign}{im_part}", format_value(re))
}

// ---------------------------------------------------------------------------
// tf(num, den) → num(s)/den(s)
// ---------------------------------------------------------------------------
//
// The slice of `cas/TransferFunction.java` the LaTeX renderer exercises:
// `LatexConverter` calls `TransferFunction.expandCalls(e, "s")` on a `tf`
// call node, which routes straight to `expandTfCall`. Coefficients are in
// descending powers, array-language-style: `[1, 3, 2]` = `s^2 + 3s + 2`.
// Every `Err` here corresponds to a Java `IllegalArgumentException` the
// caller catches to fall back to the plain `\text{tf}(…)` form; the messages
// mirror the Java ones.

/// Expand a `tf(num, den)` call into the rational expression `num(s)/den(s)`.
fn expand_tf_call(args: &[Expr], variable: &str) -> Result<Expr, String> {
    if args.len() != 2 {
        return Err("tf expects two arguments: tf(num, den)".to_string());
    }
    let num = tf_coefficients(&args[0], "num")?;
    let den = tf_coefficients(&args[1], "den")?;
    tf_fraction(&num, &den, variable)
}

/// Evaluates an array-literal argument to constant coefficients.
fn tf_coefficients(arg: &Expr, which: &str) -> Result<Vec<f64>, String> {
    let Expr::ArrayLiteral(rows) = arg else {
        return Err(format!(
            "tf {which} must be a constant array literal, e.g. [1, 3, 2]"
        ));
    };
    // A bracket literal is built as rows of cells (ArrayLiteral of
    // ArrayLiterals). A coefficient vector is 1-D, so flatten row- or
    // column-vector nesting.
    let mut elements: Vec<&Expr> = Vec::new();
    for row in rows {
        if let Expr::ArrayLiteral(cells) = row {
            elements.extend(cells.iter());
        } else {
            elements.push(row);
        }
    }
    // Java folds each element with `Evaluator.eval(elem, Map.of())`; the
    // Rust evaluator with an empty scope is the same contract. (This is the
    // engine's pure numeric AST interpreter — it computes an `f64` from a
    // parsed expression and executes no code.)
    let empty = Scope::default();
    let mut coeffs = Vec::with_capacity(elements.len());
    for elem in elements {
        match crate::eval::eval(elem, &empty) {
            Ok(v) => coeffs.push(v),
            Err(err) => {
                return Err(format!("tf {which} coefficients must be constants: {err}"));
            }
        }
    }
    Ok(coeffs)
}

/// Builds `num/den` as a single rational expression in `variable`.
fn tf_fraction(num: &[f64], den: &[f64], variable: &str) -> Result<Expr, String> {
    if den.is_empty() {
        return Err("denominator must have at least one coefficient".to_string());
    }
    Ok(Expr::bin(
        BinOp::Div,
        tf_polynomial(num, variable),
        tf_polynomial(den, variable),
    ))
}

/// Builds a polynomial in `variable` from descending-power coefficients.
fn tf_polynomial(coeffs_descending: &[f64], variable: &str) -> Expr {
    let n = coeffs_descending.len();
    let mut poly: Option<Expr> = None;
    for (i, &c) in coeffs_descending.iter().enumerate() {
        if c == 0.0 {
            continue;
        }
        let power = n - 1 - i;
        poly = Some(tf_add_term(poly, c, power, variable));
    }
    poly.unwrap_or_else(|| Expr::num(0.0))
}

/// Appends `coeff * variable^power`, using subtraction for negative
/// coefficients so the rendered polynomial reads `s^2 + 3s - 2` rather than
/// `… + -2`.
fn tf_add_term(poly: Option<Expr>, coeff: f64, power: usize, variable: &str) -> Expr {
    let negative = coeff < 0.0;
    let term = tf_term(coeff.abs(), power, variable);
    match poly {
        None => {
            if negative {
                Expr::Neg(Box::new(term))
            } else {
                term
            }
        }
        Some(p) => Expr::bin(if negative { BinOp::Sub } else { BinOp::Add }, p, term),
    }
}

/// A single `coeff * variable^power` term, with the usual `1·`/`^1`
/// simplifications.
fn tf_term(magnitude: f64, power: usize, variable: &str) -> Expr {
    match tf_power_expr(variable, power) {
        // power == 0: a bare constant.
        None => Expr::num(magnitude),
        Some(pow) => {
            if magnitude == 1.0 {
                pow
            } else {
                Expr::bin(BinOp::Mul, Expr::num(magnitude), pow)
            }
        }
    }
}

/// `variable^power`, or `None` for power 0, or the bare variable for power 1.
fn tf_power_expr(variable: &str, power: usize) -> Option<Expr> {
    if power == 0 {
        return None;
    }
    let var = Expr::var(variable);
    if power == 1 {
        return Some(var);
    }
    Some(Expr::bin(BinOp::Pow, var, Expr::num(power as f64)))
}

// ---------------------------------------------------------------------------
// Tests — expected strings copied verbatim from LatexConverterTest.java
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{CmpOp, LogicOp};

    fn names(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn num(v: f64) -> Expr {
        Expr::num(v)
    }

    fn var(n: &str) -> Expr {
        Expr::var(n)
    }

    fn neg(e: Expr) -> Expr {
        Expr::Neg(Box::new(e))
    }

    fn imag(v: f64) -> Expr {
        Expr::Num {
            value: v,
            unit: None,
            is_imaginary: true,
        }
    }

    fn residue(residues: &[[f64; 2]], poles: &[[f64; 2]], orders: &[i32], k: f64) -> ResidueResult {
        ResidueResult {
            residues: residues.to_vec(),
            poles: poles.to_vec(),
            orders: orders.to_vec(),
            k,
        }
    }

    // Java: testToLatexNumbers
    #[test]
    fn to_latex_numbers() {
        assert_eq!(expr_to_latex(&num(5.0)), "5");

        let with_unit = Expr::Num {
            value: 2.5,
            unit: Some("m".to_string()),
            is_imaginary: false,
        };
        assert_eq!(expr_to_latex(&with_unit), "2.5\\,\\left[m\\right]");

        assert_eq!(expr_to_latex(&imag(1.0)), "i");
        assert_eq!(expr_to_latex(&imag(-1.0)), "-i");
        assert_eq!(expr_to_latex(&imag(3.5)), "3.5i");

        // Java `unit.isBlank()` branch: a whitespace-only unit is dropped.
        let blank_unit = Expr::Num {
            value: 5.0,
            unit: Some(" ".to_string()),
            is_imaginary: false,
        };
        assert_eq!(expr_to_latex(&blank_unit), "5");
    }

    // Java: testToLatexVariables
    #[test]
    fn to_latex_variables() {
        assert_eq!(expr_to_latex(&var("x")), "x");
        assert_eq!(expr_to_latex(&var("x_dot")), "\\dot{x}");
        assert_eq!(expr_to_latex(&var("x_hat")), "\\hat{x}");
        assert_eq!(expr_to_latex(&var("x_1")), "x_{1}");
        assert_eq!(expr_to_latex(&var("x_1_dot")), "\\dot{x_{1}}");
        assert_eq!(expr_to_latex(&var("x_1_hat")), "\\hat{x_{1}}");
    }

    // Java: testToLatexNegativeAndBinary (the `%` assertThrows case is
    // unrepresentable: `BinOp` is a closed enum with no unknown operator).
    #[test]
    fn to_latex_negative_and_binary() {
        assert_eq!(expr_to_latex(&neg(var("x"))), "-x");

        let neg_sum = neg(Expr::bin(BinOp::Add, var("x"), var("y")));
        assert_eq!(expr_to_latex(&neg_sum), "-\\left(x + y\\right)");

        let add = Expr::bin(BinOp::Add, var("x"), var("y"));
        assert_eq!(expr_to_latex(&add), "x + y");

        let sub = Expr::bin(BinOp::Sub, var("x"), var("y"));
        assert_eq!(expr_to_latex(&sub), "x - y");

        let mul_num = Expr::bin(BinOp::Mul, num(2.0), var("y"));
        assert_eq!(expr_to_latex(&mul_num), "2\\,y");

        let mul_var = Expr::bin(BinOp::Mul, var("x"), var("y"));
        assert_eq!(expr_to_latex(&mul_var), "x\\cdot y");

        let div = Expr::bin(BinOp::Div, var("x"), var("y"));
        assert_eq!(expr_to_latex(&div), "\\frac{x}{y}");

        let pow = Expr::bin(BinOp::Pow, var("x"), num(2.0));
        assert_eq!(expr_to_latex(&pow), "x^{2}");

        let pow_neg = Expr::bin(BinOp::Pow, neg(var("x")), num(2.0));
        assert_eq!(expr_to_latex(&pow_neg), "\\left(-x\\right)^{2}");
    }

    // Rust-specific: Java throws IllegalStateException for these operators;
    // the port renders the engine's element-wise glyphs instead (see module
    // docs).
    #[test]
    fn element_wise_and_left_division_render_glyphs() {
        let case = |op: BinOp| expr_to_latex(&Expr::bin(op, var("x"), var("y")));
        assert_eq!(case(BinOp::LeftDiv), "x\\backslash y");
        assert_eq!(case(BinOp::ElemMul), "x\\odot y");
        assert_eq!(case(BinOp::ElemDiv), "x\\oslash y");
        assert_eq!(case(BinOp::ElemLeftDiv), "x\\setminus y");
        assert_eq!(case(BinOp::ElemPow), "x\\uparrow y");
    }

    // Java: testToLatexCalls
    #[test]
    fn to_latex_calls() {
        let one = |f: &str| Expr::call(f, vec![var("x")]);

        assert_eq!(expr_to_latex(&one("sqrt")), "\\sqrt{x}");
        assert_eq!(expr_to_latex(&one("sin")), "\\sin\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("cos")), "\\cos\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("tan")), "\\tan\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("asin")), "\\arcsin\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("acos")), "\\arccos\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("atan")), "\\arctan\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("ln")), "\\ln\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("log10")), "\\log_{10}\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("exp")), "e^{x}");
        assert_eq!(expr_to_latex(&one("abs")), "\\left|x\\right|");

        let convert = Expr::call("convert", vec![var("x"), var("y")]);
        assert_eq!(
            expr_to_latex(&convert),
            "\\text{Convert}\\left(x, y\\right)"
        );

        assert_eq!(
            expr_to_latex(&one("custom")),
            "\\text{custom}\\left(x\\right)"
        );

        let prop = Expr::call("prop$enthalpy$r134a$t$x", vec![var("T"), var("x")]);
        assert_eq!(
            expr_to_latex_named(&prop, &names(&[("t", "T")])),
            "\\text{Enthalpy}\\left(\\mathrm{r134a}, t=T, x=x\\right)"
        );
    }

    // Java: testToLatexArrayAndMisc
    #[test]
    fn to_latex_array_and_misc() {
        let arr = Expr::ArrayAccess {
            name: "a".to_string(), // Java's compact constructor lowercases "A"
            indices: vec![num(1.0), num(2.0)],
        };
        assert_eq!(expr_to_latex_named(&arr, &names(&[("a", "A")])), "A_{1, 2}");

        let rng = Expr::Range {
            start: Box::new(num(1.0)),
            end: Box::new(num(5.0)),
        };
        assert_eq!(expr_to_latex(&rng), "1\\dots5");

        let lit = Expr::ArrayLiteral(vec![num(1.0), num(2.0)]);
        assert_eq!(expr_to_latex(&lit), "\\left[1, 2\\right]");

        let cmp = Expr::Compare {
            op: CmpOp::Le,
            left: Box::new(var("x")),
            right: Box::new(var("y")),
        };
        assert_eq!(expr_to_latex(&cmp), "x <= y");

        let logical = Expr::Logical {
            op: LogicOp::And,
            left: Box::new(var("x")),
            right: Box::new(var("y")),
        };
        assert_eq!(expr_to_latex(&logical), "x \\text{ and } y");

        assert_eq!(expr_to_latex(&Expr::Not(Box::new(var("x")))), "\\neg x");

        let eq = Equation::new(var("x"), num(5.0), "x=5");
        assert_eq!(equation_to_latex(&eq), "x = 5");
    }

    // Java: testToLatexString
    #[test]
    fn to_latex_string() {
        assert_eq!(
            expr_to_latex(&Expr::Str("hello".to_string())),
            "\\text{'hello'}"
        );
    }

    // Java: testPowerWithBinopBaseWrapsInParens
    #[test]
    fn power_with_binop_base_wraps_in_parens() {
        let pow = Expr::bin(
            BinOp::Pow,
            Expr::bin(BinOp::Add, var("x"), var("y")),
            num(2.0),
        );
        assert_eq!(expr_to_latex(&pow), "\\left(x + y\\right)^{2}");
    }

    // Java: testHyperbolicAndInverseHyperbolicCalls
    #[test]
    fn hyperbolic_and_inverse_hyperbolic_calls() {
        let one = |f: &str| Expr::call(f, vec![var("x")]);
        assert_eq!(expr_to_latex(&one("sinh")), "\\sinh\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("cosh")), "\\cosh\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("tanh")), "\\tanh\\left(x\\right)");
        assert_eq!(
            expr_to_latex(&one("arcsinh")),
            "\\text{arcsinh}\\left(x\\right)"
        );
        assert_eq!(
            expr_to_latex(&one("arccosh")),
            "\\text{arccosh}\\left(x\\right)"
        );
        assert_eq!(
            expr_to_latex(&one("arctanh")),
            "\\text{arctanh}\\left(x\\right)"
        );
    }

    // Java: testBesselAndChiSquareCalls
    #[test]
    fn bessel_and_chi_square_calls() {
        let two = |f: &str| Expr::call(f, vec![var("x"), var("n")]);
        let one = |f: &str| Expr::call(f, vec![var("x")]);

        assert_eq!(expr_to_latex(&two("besselj")), "J_{n}\\left(x\\right)");
        assert_eq!(expr_to_latex(&two("bessel_i")), "I_{n}\\left(x\\right)");
        assert_eq!(expr_to_latex(&two("bessely")), "Y_{n}\\left(x\\right)");
        assert_eq!(expr_to_latex(&two("besselk")), "K_{n}\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("besselj0")), "J_0\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("besseli1")), "I_1\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("bessely0")), "Y_0\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("besselk1")), "K_1\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("chi_square")), "\\chi^2\\left(x\\right)");
    }

    // Coverage for the spellings LatexConverterTest leaves untested.
    #[test]
    fn remaining_call_spellings() {
        let one = |f: &str| Expr::call(f, vec![var("x")]);
        assert_eq!(expr_to_latex(&one("cbrt")), "\\sqrt[3]{x}");
        assert_eq!(expr_to_latex(&one("log2")), "\\log_{2}\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("arcsin")), "\\arcsin\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("arccos")), "\\arccos\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("arctan")), "\\arctan\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("besselj1")), "J_1\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("besseli0")), "I_0\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("bessely1")), "Y_1\\left(x\\right)");
        assert_eq!(expr_to_latex(&one("besselk0")), "K_0\\left(x\\right)");
        // Known function, missing arguments: Java throws IndexOutOfBounds;
        // the port falls back to the generic call form.
        assert_eq!(
            expr_to_latex(&Expr::call("sqrt", vec![])),
            "\\text{sqrt}\\left(\\right)"
        );
    }

    // Java: testChemistryPropertyCallWithoutEncodedArgs
    #[test]
    fn chemistry_property_call_without_encoded_args() {
        // prop$molarmass has fewer than 3 '$'-parts → rendered straight from args.
        let prop = Expr::call("prop$molarmass", vec![Expr::Str("H2O".to_string())]);
        assert_eq!(
            expr_to_latex(&prop),
            "\\text{Molarmass}\\left(\\text{'H2O'}\\right)"
        );
    }

    // Java: testTransferFunctionFallbackOnNonConstantCoeffs
    #[test]
    fn transfer_function_fallback_on_non_constant_coeffs() {
        // tf with symbolic (non-array-literal) coefficients can't expand → plain call form.
        let tf = Expr::call("tf", vec![var("a"), var("b")]);
        assert_eq!(expr_to_latex(&tf), "\\text{tf}\\left(a, b\\right)");
    }

    // Rust-specific: the expansion path the Java test file leaves uncovered,
    // pinned to TransferFunction.java's construction rules.
    #[test]
    fn transfer_function_expands_constant_coefficients() {
        let tf = Expr::call(
            "tf",
            vec![
                Expr::ArrayLiteral(vec![num(1.0), num(3.0)]),
                Expr::ArrayLiteral(vec![num(1.0), num(3.0), num(2.0)]),
            ],
        );
        assert_eq!(expr_to_latex(&tf), "\\frac{s + 3}{s^{2} + 3\\,s + 2}");
    }

    #[test]
    fn transfer_function_expansion_shapes() {
        // Row-nested literal ([[1, 3]]) flattens like the Java coefficient
        // reader; constant expressions fold through the evaluator.
        let nested = Expr::call(
            "tf",
            vec![
                Expr::ArrayLiteral(vec![Expr::ArrayLiteral(vec![num(1.0), num(3.0)])]),
                Expr::ArrayLiteral(vec![num(1.0), num(2.0)]),
            ],
        );
        assert_eq!(expr_to_latex(&nested), "\\frac{s + 3}{s + 2}");

        let folded = Expr::call(
            "tf",
            vec![
                Expr::ArrayLiteral(vec![Expr::bin(BinOp::Mul, num(2.0), num(3.0))]),
                Expr::ArrayLiteral(vec![num(1.0)]),
            ],
        );
        assert_eq!(expr_to_latex(&folded), "\\frac{6}{1}");

        // Negative coefficients render through subtraction / leading Neg.
        let negative = Expr::call(
            "tf",
            vec![
                Expr::ArrayLiteral(vec![num(1.0), num(-2.0)]),
                Expr::ArrayLiteral(vec![num(-1.0), num(0.0)]),
            ],
        );
        assert_eq!(expr_to_latex(&negative), "\\frac{s - 2}{-s}");

        // Zero-length denominator → Java IllegalArgumentException → fallback.
        let empty_den = Expr::call(
            "tf",
            vec![
                Expr::ArrayLiteral(vec![num(1.0)]),
                Expr::ArrayLiteral(vec![]),
            ],
        );
        assert_eq!(
            expr_to_latex(&empty_den),
            "\\text{tf}\\left(\\left[1\\right], \\left[\\right]\\right)"
        );

        // An all-zero numerator collapses to the literal 0.
        let zero_num = Expr::call(
            "tf",
            vec![
                Expr::ArrayLiteral(vec![num(0.0)]),
                Expr::ArrayLiteral(vec![num(1.0), num(4.0)]),
            ],
        );
        assert_eq!(expr_to_latex(&zero_num), "\\frac{0}{s + 4}");
    }

    // Rust-specific: formatValue / Double.toString parity edges.
    #[test]
    fn number_formatting_matches_java() {
        assert_eq!(expr_to_latex(&num(-2.5)), "-2.5");
        assert_eq!(expr_to_latex(&num(0.0025)), "0.0025");
        assert_eq!(expr_to_latex(&num(-0.0)), "0"); // (long) -0.0 == 0
        assert_eq!(expr_to_latex(&num(0.0009)), "9.0E-4");
        assert_eq!(expr_to_latex(&num(12_345_678.5)), "1.23456785E7");
        assert_eq!(expr_to_latex(&num(1e300)), "1.0E300");
        assert_eq!(expr_to_latex(&num(-9e-4)), "-9.0E-4");
        assert_eq!(expr_to_latex(&num(f64::NAN)), "NaN");
        assert_eq!(expr_to_latex(&num(f64::INFINITY)), "Infinity");
        assert_eq!(expr_to_latex(&num(f64::NEG_INFINITY)), "-Infinity");
        // Shortest-repr digits, same as Java 19+ Double.toString.
        assert_eq!(expr_to_latex(&num(0.1 + 0.2)), "0.30000000000000004");
        // Extra edges cross-checked against `String.valueOf(double)` on a
        // real JVM (Corretto 26): decimal/scientific boundary, shortest-digit
        // ties, and the `(long)`-cast integral check near 2^53.
        assert_eq!(expr_to_latex(&num(0.001)), "0.001");
        assert_eq!(expr_to_latex(&num(0.00099999)), "9.9999E-4");
        assert_eq!(expr_to_latex(&num(9_999_999.5)), "9999999.5");
        assert_eq!(expr_to_latex(&num(1.23e-10)), "1.23E-10");
        assert_eq!(expr_to_latex(&num(-4.56e22)), "-4.56E22");
        assert_eq!(expr_to_latex(&num(7.0 / 3.0)), "2.3333333333333335");
        assert_eq!(expr_to_latex(&num(1e16 + 2.0)), "10000000000000002");
        // `(long)` / `as i64` saturation parity around Long.MAX_VALUE.
        assert_eq!(expr_to_latex(&num(1e18)), "1000000000000000000");
        assert_eq!(expr_to_latex(&num(1e19)), "1.0E19");
        assert_eq!(expr_to_latex(&num(9.3e18)), "9.3E18");
        assert_eq!(
            expr_to_latex(&num(9.223372036854776e18)),
            "9223372036854775807"
        );
    }

    // Rust-specific: display-name pass-through, subscript conventions.
    #[test]
    fn display_names_flow_through_variables_and_equations() {
        let eq = Equation::new(var("T_in"), num(300.0), "T_in = 300");
        assert_eq!(
            equation_to_latex_named(&eq, &names(&[("t_in", "T_in")])),
            "T_{in} = 300"
        );
        // Without a display map the stored lowercase spelling renders.
        assert_eq!(equation_to_latex(&eq), "t_{in} = 300");
        // Decorations survive a display spelling.
        assert_eq!(
            expr_to_latex_named(&var("m_dot"), &names(&[("m_dot", "M_dot")])),
            "\\dot{M}"
        );
    }

    // --- Partial-fraction (residue) rendering ------------------------------

    // Java: testResidueRealPolesSum
    #[test]
    fn residue_real_poles_sum() {
        // 2/(s+1) + (-1)/(s+2)
        let res = residue(
            &[[2.0, 0.0], [-1.0, 0.0]],
            &[[-1.0, 0.0], [-2.0, 0.0]],
            &[1, 1],
            0.0,
        );
        assert_eq!(
            residue_to_latex(&res),
            "\\frac{2}{s + 1} + \\frac{-1}{s + 2}"
        );
    }

    // Java: testResidueRepeatedPolePlusDirectTerm
    #[test]
    fn residue_repeated_pole_plus_direct_term() {
        // 1/(s-3)^2 + 5
        let res = residue(&[[1.0, 0.0]], &[[3.0, 0.0]], &[2], 5.0);
        assert_eq!(
            residue_to_latex(&res),
            "\\frac{1}{\\left(s - 3\\right)^{2}} + 5"
        );
    }

    // Java: testResidueComplexPoleAndResidue
    #[test]
    fn residue_complex_pole_and_residue() {
        // 2i / (s + (1 - 3i))  from residue (0,2i), pole (-1,3i)
        let res = residue(&[[0.0, 2.0]], &[[-1.0, 3.0]], &[1], 0.0);
        assert_eq!(
            residue_to_latex(&res),
            "\\frac{2i}{s + \\left(1 - 3i\\right)}"
        );
    }

    // Java: testResidueAtOriginWithNegImaginaryResidue
    #[test]
    fn residue_at_origin_with_neg_imaginary_residue() {
        // -i / s  (residue (0,-1), pole at origin)
        let res = residue(&[[0.0, -1.0]], &[[0.0, 0.0]], &[1], 0.0);
        assert_eq!(residue_to_latex(&res), "\\frac{-i}{s}");
    }

    // Java: testResidueSkipsZeroResiduesAndShowsLoneDirectTerm
    #[test]
    fn residue_skips_zero_residues_and_shows_lone_direct_term() {
        // All residues zero → only the direct term k remains.
        let res = residue(&[[0.0, 0.0]], &[[-1.0, 0.0]], &[1], 0.0);
        assert_eq!(residue_to_latex(&res), "0");
    }

    // Java: testResidueWithFullComplexResidue
    #[test]
    fn residue_with_full_complex_residue() {
        // residue (2 + i) over a pole at origin exercises the full re+im
        // formatComplex branch.
        let res = residue(&[[2.0, 1.0]], &[[0.0, 0.0]], &[1], 0.0);
        let latex = residue_to_latex(&res);
        assert!(latex.contains("2 + i"), "{latex}");
    }

    // Rust-specific: negative direct term and a right-half-plane complex pole.
    #[test]
    fn residue_negative_direct_term_and_positive_complex_pole() {
        let res = residue(&[[1.0, 0.0]], &[[0.0, 0.0]], &[1], -2.0);
        assert_eq!(residue_to_latex(&res), "\\frac{1}{s} - 2");

        // Pole (1, 3): s - (1 + 3i).
        let rhp = residue(&[[1.0, 0.0]], &[[1.0, 3.0]], &[1], 0.0);
        assert_eq!(
            residue_to_latex(&rhp),
            "\\frac{1}{s - \\left(1 + 3i\\right)}"
        );
    }
}
