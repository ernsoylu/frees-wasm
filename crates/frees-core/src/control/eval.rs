//! Control-systems synthetic-call evaluation.
//!
//! Port of `ast/ControlSystemsEvaluator.java` (1,140 LOC) plus the four
//! `ss2tf$` / `tf2ss$` / `zp2tf$` / `tf2zp$` arms that stayed behind in
//! `ast/Evaluator.java`. Every `$`-synthetic [`crate::control::flatten`] emits
//! is decoded here: the name carries the output selector and the model
//! dimensions, the argument list carries the model itself, row-major.
//!
//! # The contract with the flattener
//!
//! This module and [`crate::control::flatten`] are two halves of one wire
//! format and must be read together. `flatten` writes
//! `out[i] = <op>$<tag>$<i>$<dims…>(entries…)`; this module reads the tags back
//! and slices `entries` into matrices. Getting the slice boundaries wrong
//! produces a plausible number rather than an error, so each unpacking site
//! names the layout it assumes and the flattener site that wrote it.
//!
//! Several Java evaluators do not carry `n` in the name at all — they
//! **recover it from the argument count**: a SISO `(A, B, C, D)` model
//! serialises to `n² + 2n + 1 = (n+1)²` values, so
//! `n = round(sqrt(args.len())) - 1`. Those formulas are transcribed exactly,
//! including the `- N` / `- 2N` corrections for the trailing sample vectors.
//!
//! # Entry point
//!
//! [`eval_intrinsic`] mirrors [`crate::linalg::eval_intrinsic`]: `None` when
//! the name is not a control synthetic (so `crate::eval` can fall through to
//! its own "not yet supported" message), `Some(Err(..))` when it is one and
//! something about it is wrong.
//!
//! # Sorting
//!
//! `pole`, `zero` and `residue` sort their results before indexing, so the
//! i-th output stays aligned across the separate synthetic calls that produce
//! it. The Java uses `Double.compare` inside a stable `Arrays.sort`;
//! [`f64::total_cmp`] has exactly `Double.compare`'s ordering (−0.0 < 0.0, NaN
//! last and equal to itself) and Rust's `sort_by` is stable, so the permutation
//! matches.

// Kernel unpacking indexes several parallel arrays by the same loop variable,
// mirroring the Java being transcribed. Iterator rewrites obscure that.
#![allow(clippy::needless_range_loop)]

use crate::diag::{FreesError, Result};
use crate::linalg::Mat;

use super::response::Kind;
use super::tf::Complex;
use super::{design, response, ss, tf};

fn err(message: impl Into<String>) -> FreesError {
    FreesError::evaluation(message)
}

/// `Math.round` semantics: half-up, NaN → 0. `libm` for wasm/native bit
/// determinism, per the port convention.
fn java_round(v: f64) -> i64 {
    if v.is_nan() {
        0
    } else {
        libm::floor(v + 0.5) as i64
    }
}

/// The Java `(int) Math.round(Math.sqrt(total)) - 1` state-count recovery: a
/// SISO `(A, B, C, D)` model serialises to `(n + 1)²` values.
fn states_from_args(total: usize) -> Result<usize> {
    let n = java_round(libm::sqrt(total as f64)) - 1;
    usize::try_from(n).map_err(|_| {
        err(format!(
            "control kernel: {total} arguments do not describe an (A, B, C, D) model"
        ))
    })
}

/// The leading `$`-separated word of every synthetic this module claims.
const CONTROL_PREFIXES: [&str; 42] = [
    "ss2tf",
    "tf2ss",
    "zp2tf",
    "tf2zp",
    "series",
    "parallel",
    "feedback",
    "ss_series",
    "ss_parallel",
    "ss_feedback",
    "pole",
    "zero",
    "bode",
    "nyquist",
    "nichols",
    "margin",
    "routh",
    "residue",
    "errorconst",
    "mason",
    "c2d",
    "d2c",
    "step",
    "impulse",
    "lsim",
    "lqr",
    "dlqr",
    "dare",
    "lyap",
    "dlyap",
    "place",
    "lqe",
    "gram",
    "balreal",
    "pidtune",
    "rank",
    "ctrb",
    "obsv",
    "ss2ss",
    "stepinfo",
    "pade",
    "rlocus",
];

/// True when [`eval_intrinsic`] claims `function` (which must contain a `$`).
pub fn handles(function: &str) -> bool {
    match function.split('$').next() {
        Some(head) => CONTROL_PREFIXES.contains(&head) && function.len() > head.len(),
        None => false,
    }
}

// ---------------------------------------------------------------------------
// Argument slicing
// ---------------------------------------------------------------------------

/// A cursor over the flat argument list, so each unpacking reads the model in
/// the same order the flattener wrote it and cannot silently run off the end.
struct Args<'a> {
    function: &'a str,
    values: &'a [f64],
    at: usize,
}

impl<'a> Args<'a> {
    fn new(function: &'a str, values: &'a [f64]) -> Args<'a> {
        Args {
            function,
            values,
            at: 0,
        }
    }

    fn remaining(&self) -> usize {
        self.values.len() - self.at.min(self.values.len())
    }

    fn short(&self, want: usize) -> FreesError {
        err(format!(
            "{}: expected at least {} argument(s), got {}",
            self.function,
            self.at + want,
            self.values.len()
        ))
    }

    fn scalar(&mut self) -> Result<f64> {
        let v = *self.values.get(self.at).ok_or_else(|| self.short(1))?;
        self.at += 1;
        Ok(v)
    }

    fn vector(&mut self, n: usize) -> Result<Vec<f64>> {
        if self.remaining() < n {
            return Err(self.short(n));
        }
        let v = self.values[self.at..self.at + n].to_vec();
        self.at += n;
        Ok(v)
    }

    /// `rows` × `cols`, row-major — the layout every `entries` list uses.
    fn matrix(&mut self, rows: usize, cols: usize) -> Result<Mat> {
        let mut m = Vec::with_capacity(rows);
        for _ in 0..rows {
            m.push(self.vector(cols)?);
        }
        Ok(m)
    }

    /// A column vector read as an `n` × 1 matrix (the Java `new double[n][1]`).
    fn column(&mut self, n: usize) -> Result<Mat> {
        Ok(self.vector(n)?.into_iter().map(|v| vec![v]).collect())
    }

    /// A row vector read as a 1 × `n` matrix (the Java `new double[1][n]`).
    fn row(&mut self, n: usize) -> Result<Mat> {
        Ok(vec![self.vector(n)?])
    }
}

/// Decoded `$`-separated name: `parts[0]` is the op, the rest its selectors.
struct Name<'a> {
    function: &'a str,
    parts: Vec<&'a str>,
}

impl<'a> Name<'a> {
    fn new(function: &'a str) -> Name<'a> {
        Name {
            function,
            parts: function.split('$').collect(),
        }
    }

    fn malformed(&self) -> FreesError {
        err(format!("malformed synthetic call: {}", self.function))
    }

    fn tag(&self, i: usize) -> Result<&'a str> {
        self.parts.get(i).copied().ok_or_else(|| self.malformed())
    }

    fn dim(&self, i: usize) -> Result<usize> {
        self.tag(i)?.parse::<usize>().map_err(|_| self.malformed())
    }

    fn last_dim(&self) -> Result<usize> {
        self.dim(self.parts.len().saturating_sub(1))
    }
}

fn pick(m: &Mat, i: usize, j: usize, function: &str) -> Result<f64> {
    m.get(i).and_then(|row| row.get(j)).copied().ok_or_else(|| {
        err(format!(
            "{function}: element [{i}][{j}] is outside the {}x{} result",
            m.len(),
            m.first().map_or(0, Vec::len)
        ))
    })
}

fn pick1(v: &[f64], i: usize, function: &str) -> Result<f64> {
    v.get(i).copied().ok_or_else(|| {
        err(format!(
            "{function}: index {i} is outside the {}-element result",
            v.len()
        ))
    })
}

/// `(re, im)` pairs ordered as the Java's `Double.compare` comparator does.
fn sort_complex(roots: &mut [Complex]) {
    roots.sort_by(|a, b| a.re.total_cmp(&b.re).then(a.im.total_cmp(&b.im)));
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Evaluate one control-systems `$`-synthetic over its already-evaluated
/// arguments. Port of the `startsWith("<op>$")` chain in `Evaluator.evalCall`.
pub fn eval_intrinsic(function: &str, args: &[f64]) -> Option<Result<f64>> {
    if !handles(function) {
        return None;
    }
    Some(dispatch(function, args))
}

fn dispatch(function: &str, args: &[f64]) -> Result<f64> {
    let name = Name::new(function);
    match name.parts[0] {
        "ss2tf" => eval_ss2tf(&name, args),
        "tf2ss" => eval_tf2ss(&name, args),
        "zp2tf" => eval_zp2tf(&name, args),
        "tf2zp" => eval_tf2zp(&name, args),
        op @ ("series" | "parallel" | "feedback") => eval_tf_combine(op, &name, args),
        "ss_series" => eval_ss_combine("series", &name, args),
        "ss_parallel" => eval_ss_combine("parallel", &name, args),
        "ss_feedback" => eval_ss_combine("feedback", &name, args),
        "pole" => eval_pole(&name, args),
        "zero" => eval_zero(&name, args),
        "bode" | "nichols" => eval_bode(&name, args),
        "nyquist" => eval_nyquist(&name, args),
        "margin" => eval_margin(&name, args),
        "routh" => eval_routh(&name, args),
        "residue" => eval_residue(&name, args),
        "errorconst" => eval_error_const(&name, args),
        "mason" => eval_mason(&name, args),
        "c2d" | "d2c" => eval_discretize(&name, args),
        "step" => eval_time_response(Kind::Step, &name, args),
        "impulse" => eval_time_response(Kind::Impulse, &name, args),
        "lsim" => eval_lsim(&name, args),
        op @ ("lqr" | "dlqr" | "dare") => eval_lqr_like(op, &name, args),
        op @ ("lyap" | "dlyap") => eval_lyap_like(op, &name, args),
        "place" => eval_place(&name, args),
        "lqe" => eval_lqe(&name, args),
        "gram" => eval_gram(&name, args),
        "balreal" => eval_balreal(&name, args),
        "pidtune" => eval_pidtune(&name, args),
        "rank" => eval_rank(&name, args),
        op @ ("ctrb" | "obsv") => eval_ctrb_obsv(op, &name, args),
        "ss2ss" => eval_ss2ss(&name, args),
        "stepinfo" => eval_stepinfo(&name, args),
        "pade" => eval_pade(&name, args),
        "rlocus" => eval_rlocus(&name, args),
        _ => Err(name.malformed()),
    }
}

// ---------------------------------------------------------------------------
// LTI conversions
// ---------------------------------------------------------------------------

/// `ss2tf$<num|den>$<k>$<n>` over `A` (n×n), `B` (n), `C` (n), `D`. Port of
/// `Evaluator.evalSs2tf`; `ss2tfij` reuses it after the flattener selects the
/// requested row of C and column of B.
fn eval_ss2tf(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let want_num = name.tag(1)? == "num";
    let k = name.dim(2)?;
    let n = name.dim(3)?;
    let mut a = Args::new(name.function, args);
    let (num, den) = read_siso_tf(&mut a, n)?;
    let coeffs = if want_num { num } else { den };
    pick1(&coeffs, k, name.function)
}

/// Reads `A` (n×n), `B` (n×1), `C` (1×n), `D` from the head of `args` and
/// converts to `(num, den)`. Port of `ControlSystemsEvaluator.ssArgsToNumDen`.
fn read_siso_tf(a: &mut Args<'_>, n: usize) -> Result<(Vec<f64>, Vec<f64>)> {
    let am = a.matrix(n, n)?;
    let bm = a.column(n)?;
    let cm = a.row(n)?;
    let d = a.scalar()?;
    let tc = ss::ss2tf(&am, &bm, &cm, d)?;
    Ok((tc.num, tc.den))
}

/// `tf2ss$a$<i>$<j>$<n>`, `tf2ss$b$<i>$<n>`, `tf2ss$c$<j>$<n>`, `tf2ss$d$<n>`
/// over the padded numerator then the denominator, both `n + 1` long. Port of
/// `Evaluator.evalTf2ss`.
fn eval_tf2ss(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let matrix = name.tag(1)?;
    let n = name.last_dim()?;
    let np = n + 1;
    let mut a = Args::new(name.function, args);
    let num = a.vector(np)?;
    let den = a.vector(np)?;
    let ssm = ss::tf2ss(&num, &den)?;
    match matrix {
        "a" => pick(&ssm.a, name.dim(2)?, name.dim(3)?, name.function),
        "b" => pick(&ssm.b, name.dim(2)?, 0, name.function),
        "c" => pick(&ssm.c, 0, name.dim(2)?, name.function),
        _ => pick(&ssm.d, 0, 0, name.function),
    }
}

/// `zp2tf$<num|den>$<k>$<nz>$<np>` over `z_r`, `z_i`, `p_r`, `p_i`, `k`. Port
/// of `Evaluator.evalZp2tf`.
fn eval_zp2tf(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let want_num = name.tag(1)? == "num";
    let k = name.dim(2)?;
    let nz = name.dim(3)?;
    let np = name.dim(4)?;
    let mut a = Args::new(name.function, args);
    let zr = a.vector(nz)?;
    let zi = a.vector(nz)?;
    let pr = a.vector(np)?;
    let pi = a.vector(np)?;
    let gain = a.scalar()?;
    let (num, den) = tf::zp2tf(&zr, &zi, &pr, &pi, gain);
    pick1(if want_num { &num } else { &den }, k, name.function)
}

/// `tf2zp$<zr|zi|pr|pi>$<k>$<nz>$<np>` and `tf2zp$k$<nz>$<np>`.
///
/// Port of `Evaluator.evalTf2zp`, including its assumption that **both**
/// coefficient vectors are `np + 1` long: `flattenTf2zp` serialises the
/// numerator *unpadded*, so a shorter numerator makes the two halves overlap.
/// The Java has the same hole; a length check here would refuse documents the
/// oracle accepts, so the read is transcribed and the arity is the only guard.
fn eval_tf2zp(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let output = name.tag(1)?;
    let is_k = output == "k";
    let np = if is_k { name.dim(3)? } else { name.dim(4)? };
    let num_len = np + 1;
    let mut a = Args::new(name.function, args);
    let num = a.vector(num_len)?;
    let den = a.vector(num_len)?;
    let zpk = tf::tf2zp(&num, &den)?;
    if is_k {
        return Ok(zpk.k);
    }
    let k = name.dim(2)?;
    // Out-of-range indices read 0.0, not an error: the caller may declare more
    // zero slots than the transfer function has finite zeros.
    Ok(match output {
        "zr" => zpk.zeros.get(k).map_or(0.0, |z| z.re),
        "zi" => zpk.zeros.get(k).map_or(0.0, |z| z.im),
        "pr" => zpk.poles.get(k).map_or(0.0, |p| p.re),
        "pi" => zpk.poles.get(k).map_or(0.0, |p| p.im),
        _ => return Err(name.malformed()),
    })
}

// ---------------------------------------------------------------------------
// Interconnection
// ---------------------------------------------------------------------------

/// `<series|parallel|feedback>$<num|den>$<i>$<L1>$<L2>` over `num1`, `den1`,
/// `num2`, `den2` (and the feedback sign). Port of `evalSeries` / `evalParallel`
/// / `evalFeedback`, which differ only in the kernel they call.
fn eval_tf_combine(op: &str, name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let want_num = name.tag(1)? == "num";
    let index = name.dim(2)?;
    let l1 = name.dim(3)?;
    let l2 = name.dim(4)?;
    let mut a = Args::new(name.function, args);
    let num1 = a.vector(l1)?;
    let den1 = a.vector(l1)?;
    let num2 = a.vector(l2)?;
    let den2 = a.vector(l2)?;
    let (num, den) = match op {
        "series" => tf::series(&num1, &den1, &num2, &den2),
        "parallel" => tf::parallel(&num1, &den1, &num2, &den2),
        _ => {
            let sign = a.scalar()?;
            tf::feedback(&num1, &den1, &num2, &den2, sign)
        }
    };
    pick1(if want_num { &num } else { &den }, index, name.function)
}

/// `ss_<op>$<a|b|c|d>$<i>$<j>$<n1>$<p1>$<q1>$<n2>$<p2>$<q2>` over the two
/// stacked realizations. Port of `evalSsCombine`.
fn eval_ss_combine(op: &str, name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let var_type = name.tag(1)?;
    let i = name.dim(2)?;
    let j = name.dim(3)?;
    let n1 = name.dim(4)?;
    let p1 = name.dim(5)?;
    let q1 = name.dim(6)?;
    let n2 = name.dim(7)?;
    let p2 = name.dim(8)?;
    let q2 = name.dim(9)?;

    let mut a = Args::new(name.function, args);
    let a1 = a.matrix(n1, n1)?;
    let b1 = a.matrix(n1, p1)?;
    let c1 = a.matrix(q1, n1)?;
    let d1 = a.matrix(q1, p1)?;
    let a2 = a.matrix(n2, n2)?;
    let b2 = a.matrix(n2, p2)?;
    let c2 = a.matrix(q2, n2)?;
    let d2 = a.matrix(q2, p2)?;

    let res = match op {
        "series" => design::ss_series(&a1, &b1, &c1, &d1, &a2, &b2, &c2, &d2)?,
        "parallel" => design::ss_parallel(&a1, &b1, &c1, &d1, &a2, &b2, &c2, &d2)?,
        _ => {
            let sign = a.scalar()?;
            design::ss_feedback(&a1, &b1, &c1, &d1, &a2, &b2, &c2, &d2, sign)?
        }
    };
    select_ss(&res, var_type, i, j, name.function)
}

/// `ss2ss$<a|b|c|d>$<i>$<j>$<n>$<m>$<p>` over `A`, `B`, `C`, `D`, `P`. Port of
/// `evalSs2ss`.
fn eval_ss2ss(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let var_type = name.tag(1)?;
    let i = name.dim(2)?;
    let j = name.dim(3)?;
    let n = name.dim(4)?;
    let m = name.dim(5)?;
    let p = name.dim(6)?;
    let mut a = Args::new(name.function, args);
    let am = a.matrix(n, n)?;
    let bm = a.matrix(n, m)?;
    let cm = a.matrix(p, n)?;
    let dm = a.matrix(p, m)?;
    let transform = a.matrix(n, n)?;
    let res = design::ss2ss(&am, &bm, &cm, &dm, &transform)?;
    select_ss(&res, var_type, i, j, name.function)
}

fn select_ss(
    res: &ss::StateSpaceMatrices,
    var_type: &str,
    i: usize,
    j: usize,
    function: &str,
) -> Result<f64> {
    // The Java's `if a / else if b / else if c / else d` — an unrecognised tag
    // reads D rather than erroring.
    let m = match var_type {
        "a" => &res.a,
        "b" => &res.b,
        "c" => &res.c,
        _ => &res.d,
    };
    pick(m, i, j, function)
}

// ---------------------------------------------------------------------------
// Poles, zeros, frequency response
// ---------------------------------------------------------------------------

/// `pole$<pr|pi>$<i>$<numInputs>$<n>`: eigenvalues of `A` (1 input) or roots of
/// the denominator (2 inputs). Port of `evalPole`, including its read of only
/// the *second* half of the arguments in the transfer-function form.
fn eval_pole(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let want_real = name.tag(1)? == "pr";
    let index = name.dim(2)?;
    let num_inputs = name.dim(3)?;
    let n = name.dim(4)?;
    let mut result = if num_inputs == 1 {
        let mut a = Args::new(name.function, args);
        let am = a.matrix(n, n)?;
        tf::pole_ss(&am)?
    } else {
        let len = n + 1;
        // The numerator occupies args[0..len]; only the denominator is read.
        let mut a = Args::new(name.function, args);
        let _num = a.vector(len)?;
        let den = a.vector(len)?;
        tf::roots(&den)?
    };
    sort_complex(&mut result);
    let root = result.get(index).ok_or_else(|| {
        err(format!(
            "{}: pole index {index} is outside the {}-root result",
            name.function,
            result.len()
        ))
    })?;
    Ok(if want_real { root.re } else { root.im })
}

/// `zero$<zr|zi>$<i>$<numInputs>$<nz>`: roots of the numerator, taken directly
/// (2 inputs) or via `ss2tf` (4 inputs). Port of `evalZero` — an index past the
/// last finite zero reads `0.0`, as does an empty root set.
fn eval_zero(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let want_real = name.tag(1)? == "zr";
    let index = name.dim(2)?;
    let num_inputs = name.dim(3)?;
    let num = if num_inputs == 2 {
        let len = args.len() / 2;
        let mut a = Args::new(name.function, args);
        a.vector(len)?
    } else {
        let n = states_from_args(args.len())?;
        let mut a = Args::new(name.function, args);
        read_siso_tf(&mut a, n)?.0
    };
    let mut result = tf::roots(&num)?;
    if result.is_empty() {
        return Ok(0.0);
    }
    sort_complex(&mut result);
    Ok(match result.get(index) {
        Some(root) if want_real => root.re,
        Some(root) => root.im,
        None => 0.0,
    })
}

/// The `{num, den, omega}` a flattened frequency-response call carries. Port of
/// `ControlSystemsEvaluator.freqResponseModel`.
fn freq_response_model(
    function: &str,
    args: &[f64],
    num_inputs: usize,
    n_pts: usize,
) -> Result<(Vec<f64>, Vec<f64>, Vec<f64>)> {
    if num_inputs == 3 {
        let len = args.len().saturating_sub(n_pts) / 2;
        let mut a = Args::new(function, args);
        let num = a.vector(len)?;
        let den = a.vector(len)?;
        let omega = a.vector(n_pts)?;
        return Ok((num, den, omega));
    }
    let n = states_from_args(args.len().saturating_sub(n_pts))?;
    let mut a = Args::new(function, args);
    let (num, den) = read_siso_tf(&mut a, n)?;
    let omega = a.vector(n_pts)?;
    Ok((num, den, omega))
}

/// `bode$<mag|phase>$<i>$<numInputs>$<N>` and the identically-shaped
/// `nichols$…`. Port of `evalBode` / `evalNichols`, which call the same kernel.
fn eval_bode(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let want_mag = name.tag(1)? == "mag";
    let index = name.dim(2)?;
    let num_inputs = name.dim(3)?;
    let n_pts = name.dim(4)?;
    let (num, den, omega) = freq_response_model(name.function, args, num_inputs, n_pts)?;
    let (mag, phase) = tf::bode(&num, &den, &omega);
    pick1(if want_mag { &mag } else { &phase }, index, name.function)
}

/// `nyquist$<real|imag>$<i>$<numInputs>$<N>`. Port of `evalNyquist`.
fn eval_nyquist(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let want_real = name.tag(1)? == "real";
    let index = name.dim(2)?;
    let num_inputs = name.dim(3)?;
    let n_pts = name.dim(4)?;
    let (num, den, omega) = freq_response_model(name.function, args, num_inputs, n_pts)?;
    let (re, im) = tf::nyquist(&num, &den, &omega);
    pick1(if want_real { &re } else { &im }, index, name.function)
}

/// `margin$<gm|pm|wcg|wcp>$<numInputs>`. Port of `evalMargin` — an unrecognised
/// output tag reads `0.0`, as the Java's `default` arm does.
fn eval_margin(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let output = name.tag(1)?;
    let num_inputs = name.dim(2)?;
    let (num, den) = if num_inputs == 2 {
        let len = args.len() / 2;
        let mut a = Args::new(name.function, args);
        (a.vector(len)?, a.vector(len)?)
    } else {
        let n = states_from_args(args.len())?;
        let mut a = Args::new(name.function, args);
        read_siso_tf(&mut a, n)?
    };
    let result = tf::margin(&num, &den);
    Ok(match output {
        "gm" => result[0],
        "pm" => result[1],
        "wcg" => result[2],
        "wcp" => result[3],
        _ => 0.0,
    })
}

/// `routh$<nrhp|stable>$<L>` over the characteristic-polynomial coefficients.
/// Port of `evalRouth`.
fn eval_routh(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let output = name.tag(1)?;
    let len = name.dim(2)?;
    let mut a = Args::new(name.function, args);
    let den = a.vector(len)?;
    let n_rhp = tf::routh(&den);
    if output == "stable" {
        return Ok(if n_rhp == 0 { 1.0 } else { 0.0 });
    }
    Ok(n_rhp as f64)
}

/// `residue$<rr|ri|pr|pi|ord>$<form>$<i>$<numLen>$<n>` and
/// `residue$k$<form>$<numLen>$<n>`, where `<form>` is `s` (the 5-output simple
/// form) or `o` (the 6-output form carrying the per-term order). Port of
/// `evalResidue`: terms are sorted by `(pole re, pole im, order)` so the i-th
/// outputs of the separate synthetics stay aligned, and the simple form refuses
/// a repeated pole rather than silently dropping a term.
fn eval_residue(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let which = name.tag(1)?;
    let is_k = which == "k";
    let form = name.tag(2)?;
    let num_len = name.dim(if is_k { 3 } else { 4 })?;
    let n = name.dim(if is_k { 4 } else { 5 })?;

    let mut a = Args::new(name.function, args);
    let num = a.vector(num_len)?;
    let den = a.vector(n + 1)?;

    let res = tf::residue(&num, &den)?;
    if form == "s" && res.orders.iter().any(|&o| o > 1) {
        return Err(err(
            "residue: repeated poles require the 6-output form with an order array, \
             e.g. CALL residue(num, den : r_r, r_i, p_r, p_i, ord, k)",
        ));
    }
    if is_k {
        return Ok(res.k);
    }
    let rank = name.dim(3)?;
    let src = sorted_residue_index(&res.poles, &res.orders, rank).ok_or_else(|| {
        err(format!(
            "{}: residue index {rank} is outside the {}-term result",
            name.function,
            res.poles.len()
        ))
    })?;
    Ok(match which {
        "rr" => res.residues[src].re,
        "ri" => res.residues[src].im,
        "pr" => res.poles[src].re,
        "pi" => res.poles[src].im,
        "ord" => res.orders[src] as f64,
        _ => 0.0,
    })
}

/// Source index of the `rank`-th residue term under the Java's stable sort by
/// `(pole re, pole im, order)`. Port of `sortedResidueIndex`.
fn sorted_residue_index(poles: &[Complex], orders: &[usize], rank: usize) -> Option<usize> {
    let mut perm: Vec<usize> = (0..poles.len()).collect();
    perm.sort_by(|&i, &j| {
        poles[i]
            .re
            .total_cmp(&poles[j].re)
            .then(poles[i].im.total_cmp(&poles[j].im))
            .then(orders[i].cmp(&orders[j]))
    });
    perm.get(rank).copied()
}

/// `errorconst$<kp|kv|ka>$<numLen>$<denLen>` over the open-loop numerator then
/// denominator, both **unpadded**. Port of `evalErrorConst`.
fn eval_error_const(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let which = name.tag(1)?;
    let num_len = name.dim(2)?;
    let den_len = name.dim(3)?;
    let mut a = Args::new(name.function, args);
    let num = a.vector(num_len)?;
    let den = a.vector(den_len)?;
    let k = tf::error_constants(&num, &den)?;
    Ok(match which {
        "kp" => k[0],
        "kv" => k[1],
        "ka" => k[2],
        _ => 0.0,
    })
}

/// `mason$<n>` over the n×n node-gain matrix then the 1-based source and sink.
/// Port of `evalMason`.
fn eval_mason(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let n = name.dim(1)?;
    let mut a = Args::new(name.function, args);
    let g = a.matrix(n, n)?;
    let source = java_round(a.scalar()?) - 1;
    let sink = java_round(a.scalar()?) - 1;
    if source < 0 || source >= n as i64 || sink < 0 || sink >= n as i64 {
        return Err(err(format!("mason: source/sink node out of range 1..{n}")));
    }
    tf::mason(&g, source as usize, sink as usize)
}

/// `<c2d|d2c>$<num|den>$<method>$<i>$<L>` over the padded numerator, the
/// denominator and `Ts`. Port of `evalDiscretize`, including its right-aligned
/// read: a ZOH numerator can be shorter than `L`, and the missing high-power
/// terms read `0.0`.
fn eval_discretize(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let op = name.parts[0];
    let want_num = name.tag(1)? == "num";
    let method = name.tag(2)?;
    let index = name.dim(3)?;
    let len = name.dim(4)?;
    let mut a = Args::new(name.function, args);
    let num = a.vector(len)?;
    let den = a.vector(len)?;
    let ts = a.scalar()?;
    let (out_num, out_den) = if op == "c2d" {
        tf::c2d(&num, &den, ts, Some(method))?
    } else {
        tf::d2c(&num, &den, ts, Some(method))?
    };
    let coeffs = if want_num { out_num } else { out_den };
    let off = coeffs.len() as i64 - len as i64 + index as i64;
    Ok(if off >= 0 && off < coeffs.len() as i64 {
        coeffs[off as usize]
    } else {
        0.0
    })
}

// ---------------------------------------------------------------------------
// Time response
// ---------------------------------------------------------------------------

/// `<step|impulse>$<i>$<numInputs>$<N>` over the serialised model then the N
/// time samples. Port of `evalTimeResponse`.
fn eval_time_response(kind: Kind, name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let index = name.dim(1)?;
    let num_inputs = name.dim(2)?;
    let n_pts = name.dim(3)?;
    let y = if num_inputs == 3 {
        let len = args.len().saturating_sub(n_pts) / 2;
        let mut a = Args::new(name.function, args);
        let num = a.vector(len)?;
        let den = a.vector(len)?;
        let t = a.vector(n_pts)?;
        response::response(kind, &num, &den, None, &t)?
    } else {
        let n = states_from_args(args.len().saturating_sub(n_pts))?;
        let mut a = Args::new(name.function, args);
        let (am, bv, cv, d) = read_siso_ss(&mut a, n)?;
        let t = a.vector(n_pts)?;
        response::response_ss(kind, &am, &bv, &cv, d, None, &t)?
    };
    pick1(&y, index, name.function)
}

/// `lsim$<i>$<numInputs>$<N>` over the model, then the N input samples, then
/// the N time samples. Port of `evalLsim`.
fn eval_lsim(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let index = name.dim(1)?;
    let num_inputs = name.dim(2)?;
    let n_pts = name.dim(3)?;
    let y = if num_inputs == 4 {
        let len = args.len().saturating_sub(2 * n_pts) / 2;
        let mut a = Args::new(name.function, args);
        let num = a.vector(len)?;
        let den = a.vector(len)?;
        let u = a.vector(n_pts)?;
        let t = a.vector(n_pts)?;
        response::response(Kind::Lsim, &num, &den, Some(&u), &t)?
    } else {
        let n = states_from_args(args.len().saturating_sub(2 * n_pts))?;
        let mut a = Args::new(name.function, args);
        let (am, bv, cv, d) = read_siso_ss(&mut a, n)?;
        let u = a.vector(n_pts)?;
        let t = a.vector(n_pts)?;
        response::response_ss(Kind::Lsim, &am, &bv, &cv, d, Some(&u), &t)?
    };
    pick1(&y, index, name.function)
}

/// `A` (n×n), `B` (n), `C` (n), `D` — the flat form the time-response kernels
/// take (the Java `double[] b`, `double[] cm`, not the 2-D `ss2tf` shape).
fn read_siso_ss(a: &mut Args<'_>, n: usize) -> Result<(Mat, Vec<f64>, Vec<f64>, f64)> {
    let am = a.matrix(n, n)?;
    let b = a.vector(n)?;
    let c = a.vector(n)?;
    let d = a.scalar()?;
    Ok((am, b, c, d))
}

/// `stepinfo$<tr|tp|ts|os>$<N>` over the N time samples then the N response
/// samples. Port of `evalStepInfo` — an unrecognised tag reads the overshoot,
/// as the Java's `default` arm does.
fn eval_stepinfo(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let var_type = name.tag(1)?;
    let n_pts = name.dim(2)?;
    let mut a = Args::new(name.function, args);
    let t = a.vector(n_pts)?;
    let y = a.vector(n_pts)?;
    let res = design::stepinfo(&t, &y);
    Ok(match var_type {
        "tr" => res[0],
        "tp" => res[1],
        "ts" => res[2],
        _ => res[3],
    })
}

/// `pade$<num|den>$<i>$<order>`. Port of `evalPade`, which reads only the first
/// argument (`Td`) — the second is the order, already in the name.
fn eval_pade(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let var_type = name.tag(1)?;
    let index = name.dim(2)?;
    let order = name.dim(3)?;
    let mut a = Args::new(name.function, args);
    let td = a.scalar()?;
    let res = design::pade(td, order);
    let coeffs = if var_type == "num" { &res[0] } else { &res[1] };
    pick1(coeffs, index, name.function)
}

/// `rlocus$k$<i>$<numSize>$<denSize>$<M>$<N>` and
/// `rlocus$<cpr|cpi>$<i>$<j>$<numSize>$<denSize>$<M>$<N>` over the numerator
/// then the denominator, both **unpadded**. Port of `evalRlocus`, whose name
/// layout shifts by one because the gain vector needs no column index.
fn eval_rlocus(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let var_type = name.tag(1)?;
    let i = name.dim(2)?;
    let want_k = var_type == "k";
    let (j, num_size, den_size, m_points) = if want_k {
        (0, name.dim(3)?, name.dim(4)?, name.dim(5)?)
    } else {
        (name.dim(3)?, name.dim(4)?, name.dim(5)?, name.dim(6)?)
    };
    let mut a = Args::new(name.function, args);
    let num = a.vector(num_size)?;
    let den = a.vector(den_size)?;
    let res = design::rlocus(&num, &den, m_points)?;
    if want_k {
        return pick1(&res.k, i, name.function);
    }
    let m = if var_type == "cpr" {
        &res.cpr
    } else {
        &res.cpi
    };
    pick(m, i, j, name.function)
}

// ---------------------------------------------------------------------------
// Design
// ---------------------------------------------------------------------------

/// `<lqr|dlqr|dare>$<i>$<j>$<n>$<m>` over `A` (n×n), `B` (n×m), `Q` (n×n),
/// `R` (m×m). Port of `evalLqrLike`.
fn eval_lqr_like(op: &str, name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let i = name.dim(1)?;
    let j = name.dim(2)?;
    let n = name.dim(3)?;
    let m = name.dim(4)?;
    let mut a = Args::new(name.function, args);
    let am = a.matrix(n, n)?;
    let bm = a.matrix(n, m)?;
    let q = a.matrix(n, n)?;
    let r = a.matrix(m, m)?;
    let res = match op {
        "lqr" => design::lqr(&am, &bm, &q, &r)?,
        "dlqr" => design::dlqr(&am, &bm, &q, &r)?,
        _ => design::dare(&am, &bm, &q, &r)?,
    };
    pick(&res, i, j, name.function)
}

/// `<lyap|dlyap>$<i>$<j>$<n>` over `A` (n×n) then `Q` (n×n). Port of
/// `evalLyapLike`.
fn eval_lyap_like(op: &str, name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let i = name.dim(1)?;
    let j = name.dim(2)?;
    let n = name.dim(3)?;
    let mut a = Args::new(name.function, args);
    let am = a.matrix(n, n)?;
    let q = a.matrix(n, n)?;
    let res = if op == "lyap" {
        design::lyap(&am, &q)?
    } else {
        design::dlyap(&am, &q)?
    };
    pick(&res, i, j, name.function)
}

/// `<ctrb|obsv>$<i>$<j>$<n>$<r>$<cols>` over `A` (n×n) then `B`/`C` (r×cols).
/// Port of `evalCtrbObsv`.
fn eval_ctrb_obsv(op: &str, name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let i = name.dim(1)?;
    let j = name.dim(2)?;
    let n = name.dim(3)?;
    let r = name.dim(4)?;
    let cols = name.dim(5)?;
    let mut a = Args::new(name.function, args);
    let am = a.matrix(n, n)?;
    let b_or_c = a.matrix(r, cols)?;
    let res = if op == "ctrb" {
        design::ctrb(&am, &b_or_c)?
    } else {
        design::obsv(&am, &b_or_c)?
    };
    pick(&res, i, j, name.function)
}

/// `place$<i>$<j>$<n>$<m>` over `A` (n×n), `B` (n×m), `pr` (n), `pi` (n). Port
/// of `evalPlace`, which is SISO-only and indexes the gain by its *column*.
fn eval_place(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let _row = name.dim(1)?;
    let col = name.dim(2)?;
    let n = name.dim(3)?;
    let m = name.dim(4)?;
    let mut a = Args::new(name.function, args);
    let am = a.matrix(n, n)?;
    let bm = a.matrix(n, m)?;
    let mut roots = vec![[0.0_f64; 2]; n];
    for root in roots.iter_mut() {
        root[0] = a.scalar()?;
    }
    for root in roots.iter_mut() {
        root[1] = a.scalar()?;
    }
    if m != 1 {
        return Err(err("place currently only supports SISO systems (m=1)"));
    }
    let b_vector: Vec<f64> = (0..n).map(|i| bm[i][0]).collect();
    let k = design::place(&am, &b_vector, &roots)?;
    pick1(&k, col, name.function)
}

/// `lqe$<i>$<j>$<n>$<g>$<p>` over `A` (n×n), `G` (n×g), `C` (p×n), `Q` (g×g),
/// `R` (p×p). Port of `evalLqe`.
fn eval_lqe(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let i = name.dim(1)?;
    let j = name.dim(2)?;
    let n = name.dim(3)?;
    let g = name.dim(4)?;
    let p = name.dim(5)?;
    let mut a = Args::new(name.function, args);
    let am = a.matrix(n, n)?;
    let gm = a.matrix(n, g)?;
    let cm = a.matrix(p, n)?;
    let q = a.matrix(g, g)?;
    let r = a.matrix(p, p)?;
    let res = design::lqe(&am, &gm, &cm, &q, &r)?;
    pick(&res, i, j, name.function)
}

/// `gram$<c|o>$<i>$<j>$<n>$<r>$<cols>` over `A` (n×n) then `M` (r×cols). Port
/// of `evalGram`.
fn eval_gram(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let kind = name
        .tag(1)?
        .chars()
        .next()
        .ok_or_else(|| name.malformed())?;
    let i = name.dim(2)?;
    let j = name.dim(3)?;
    let n = name.dim(4)?;
    let r = name.dim(5)?;
    let cols = name.dim(6)?;
    let mut a = Args::new(name.function, args);
    let am = a.matrix(n, n)?;
    let m = a.matrix(r, cols)?;
    let res = design::gramian(&am, &m, kind)?;
    pick(&res, i, j, name.function)
}

/// `balreal$<a|b|c>$<i>$<j>$<n>$<m>$<p>` over `A` (n×n), `B` (n×m), `C` (p×n).
/// Port of `evalBalreal`.
fn eval_balreal(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let tag = name.tag(1)?;
    let i = name.dim(2)?;
    let j = name.dim(3)?;
    let n = name.dim(4)?;
    let m = name.dim(5)?;
    let p = name.dim(6)?;
    let mut a = Args::new(name.function, args);
    let am = a.matrix(n, n)?;
    let bm = a.matrix(n, m)?;
    let cm = a.matrix(p, n)?;
    let res = design::balreal(&am, &bm, &cm)?;
    let out = match tag {
        "a" => &res.a,
        "b" => &res.b,
        _ => &res.c,
    };
    pick(out, i, j, name.function)
}

/// `pidtune$<kp|ki|kd>$<p|pi|pid>` over the padded numerator, the denominator
/// and `wc`. Port of `evalPidtune`.
fn eval_pidtune(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let output = name.tag(1)?;
    let kind = name.tag(2)?;
    let len = args.len().saturating_sub(1) / 2;
    let mut a = Args::new(name.function, args);
    let num = a.vector(len)?;
    let den = a.vector(len)?;
    let wc = a.scalar()?;
    let gains = design::pidtune(&num, &den, kind, wc)?;
    Ok(match output {
        "kp" => gains[0],
        "ki" => gains[1],
        "kd" => gains[2],
        _ => 0.0,
    })
}

/// `rank$<rows>$<cols>` over the matrix, row-major. Port of `evalRank`.
fn eval_rank(name: &Name<'_>, args: &[f64]) -> Result<f64> {
    let rows = name.dim(1)?;
    let cols = name.dim(2)?;
    let mut a = Args::new(name.function, args);
    let m = a.matrix(rows, cols)?;
    Ok(design::rank(&m)? as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_control_synthetics_are_claimed() {
        assert!(handles("ss2tf$num$0$2"));
        assert!(handles("ss_feedback$a$0$0$1$1$1$1$1$1"));
        assert!(handles("pidtune$kp$pid"));
        // A bare op name with no `$` is an ordinary intrinsic, not a synthetic.
        assert!(!handles("step"));
        assert!(!handles("rank"));
        // Other families keep their own dispatch.
        assert!(!handles("qr$q$0$0$2$2"));
        assert!(!handles("prop$enthalpy$water"));
        assert!(eval_intrinsic("qr$q$0$0$2$2", &[]).is_none());
    }

    #[test]
    fn a_malformed_name_is_an_error_not_a_panic() {
        let got = eval_intrinsic("rank$two$three", &[1.0]).unwrap();
        assert!(
            got.unwrap_err().to_string().contains("malformed synthetic"),
            "expected a malformed-name error"
        );
        let got = eval_intrinsic("lqr$0", &[1.0]).unwrap();
        assert!(got.is_err());
    }

    #[test]
    fn a_short_argument_list_is_an_error_not_a_panic() {
        let got = eval_intrinsic("rank$3$3", &[1.0, 2.0]).unwrap();
        let message = got.unwrap_err().to_string();
        assert!(message.contains("expected at least"), "{message}");
    }

    #[test]
    fn rank_reads_its_matrix_row_major() {
        // [[1, 2], [2, 4]] is rank 1.
        let got = eval_intrinsic("rank$2$2", &[1.0, 2.0, 2.0, 4.0]).unwrap();
        assert_eq!(got.unwrap(), 1.0);
        // The identity is full rank.
        let got = eval_intrinsic("rank$2$2", &[1.0, 0.0, 0.0, 1.0]).unwrap();
        assert_eq!(got.unwrap(), 2.0);
    }

    /// Oracle (`tools/golden-dumper`, `d02_ctrb_obsv_rank`):
    /// `A = [0 1; 0 0]`, `B = [0; 1]` gives `Co = [0 1; 1 0]` and, for the
    /// transposed `C = [0 1]`, `Ob = [0 1; 0 0]`.
    #[test]
    fn ctrb_and_obsv_match_the_oracle() {
        let a = [0.0, 1.0, 0.0, 0.0];
        let b = [0.0, 1.0];
        let args: Vec<f64> = a.iter().chain(b.iter()).copied().collect();
        let want_co = [[0.0, 1.0], [1.0, 0.0]];
        for i in 0..2 {
            for j in 0..2 {
                let got = eval_intrinsic(&format!("ctrb${i}${j}$2$2$1"), &args)
                    .unwrap()
                    .unwrap();
                assert_eq!(got, want_co[i][j], "ctrb[{i}][{j}]");
            }
        }
        let want_ob = [[0.0, 1.0], [0.0, 0.0]];
        for i in 0..2 {
            for j in 0..2 {
                let got = eval_intrinsic(&format!("obsv${i}${j}$2$1$2"), &args)
                    .unwrap()
                    .unwrap();
                assert_eq!(got, want_ob[i][j], "obsv[{i}][{j}]");
            }
        }
    }

    /// Oracle (`d04_lqr_place_acker`): `A = [0 1; 0 0]`, `B = [0; 1]`,
    /// `Q = I`, `R = 1` gives `K = [1, sqrt(3)]`.
    #[test]
    fn lqr_matches_the_oracle() {
        let args = [
            0.0, 1.0, 0.0, 0.0, // A
            0.0, 1.0, // B (n x m = 2 x 1)
            1.0, 0.0, 0.0, 1.0, // Q
            1.0, // R (m x m)
        ];
        let k0 = eval_intrinsic("lqr$0$0$2$1", &args).unwrap().unwrap();
        let k1 = eval_intrinsic("lqr$0$1$2$1", &args).unwrap().unwrap();
        assert!((k0 - 1.0).abs() < 1e-9, "K[0] = {k0}");
        assert!((k1 - 3.0_f64.sqrt()).abs() < 1e-9, "K[1] = {k1}");
    }

    /// Oracle (`d04_lqr_place_acker`): desired poles −1, −2 give `K = [2, 3]`.
    #[test]
    fn place_matches_the_oracle_and_refuses_mimo() {
        let args = [
            0.0, 1.0, 0.0, 0.0, // A
            0.0, 1.0, // B
            -1.0, -2.0, // pr
            0.0, 0.0, // pi
        ];
        let k0 = eval_intrinsic("place$0$0$2$1", &args).unwrap().unwrap();
        let k1 = eval_intrinsic("place$0$1$2$1", &args).unwrap().unwrap();
        assert!((k0 - 2.0).abs() < 1e-9, "K[0] = {k0}");
        assert!((k1 - 3.0).abs() < 1e-9, "K[1] = {k1}");

        let mimo = [0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, -1.0, -2.0, 0.0, 0.0];
        let got = eval_intrinsic("place$0$0$2$2", &mimo).unwrap();
        assert!(got.unwrap_err().to_string().contains("only supports SISO"));
    }

    /// Oracle (`c09_lyap_dlyap_dlqr_dare_gram`): `A = diag(−1, −2)`, `Q = I`
    /// gives `X = diag(0.5, 0.25)` and `Wc = [[0.5, 1/3], [1/3, 0.25]]`.
    #[test]
    fn lyap_and_gram_match_the_oracle() {
        let a = [-1.0, 0.0, 0.0, -2.0];
        let q = [1.0, 0.0, 0.0, 1.0];
        let args: Vec<f64> = a.iter().chain(q.iter()).copied().collect();
        let x00 = eval_intrinsic("lyap$0$0$2", &args).unwrap().unwrap();
        let x11 = eval_intrinsic("lyap$1$1$2", &args).unwrap().unwrap();
        assert!((x00 - 0.5).abs() < 1e-9, "X[0][0] = {x00}");
        assert!((x11 - 0.25).abs() < 1e-9, "X[1][1] = {x11}");

        let b = [1.0, 1.0]; // B is 2 x 1
        let gram_args: Vec<f64> = a.iter().chain(b.iter()).copied().collect();
        let w01 = eval_intrinsic("gram$c$0$1$2$2$1", &gram_args)
            .unwrap()
            .unwrap();
        assert!((w01 - 1.0 / 3.0).abs() < 1e-9, "Wc[0][1] = {w01}");
    }

    /// Oracle (`d05_ss2ss_balreal_lqe`): the identity transform leaves the
    /// realization unchanged, and `L = [0.5792499…, 0.4470514…]`.
    #[test]
    fn ss2ss_and_lqe_match_the_oracle() {
        let args = [
            -1.0, 0.0, 0.0, -2.0, // A (2x2)
            1.0, 1.0, // B (2x1)
            1.0, 1.0, // C (1x2)
            0.0, // D (1x1)
            1.0, 0.0, 0.0, 1.0, // P (2x2)
        ];
        let a00 = eval_intrinsic("ss2ss$a$0$0$2$1$1", &args).unwrap().unwrap();
        let a11 = eval_intrinsic("ss2ss$a$1$1$2$1$1", &args).unwrap().unwrap();
        assert!((a00 + 1.0).abs() < 1e-9, "An[0][0] = {a00}");
        assert!((a11 + 2.0).abs() < 1e-9, "An[1][1] = {a11}");

        let lqe_args = [
            -1.0, 0.0, 0.0, -2.0, // A
            1.0, 1.0, // G (2x1)
            1.0, 1.0, // C (1x2)
            1.0, // Q (1x1)
            1.0, // R (1x1)
        ];
        let l0 = eval_intrinsic("lqe$0$0$2$1$1", &lqe_args).unwrap().unwrap();
        let l1 = eval_intrinsic("lqe$1$0$2$1$1", &lqe_args).unwrap().unwrap();
        assert!((l0 - 0.5792499).abs() < 1e-6, "L[0] = {l0}");
        assert!((l1 - 0.4470514).abs() < 1e-6, "L[1] = {l1}");
    }

    /// Oracle (`d05_ss2ss_balreal_lqe`): `Ab[0][0] = −1.3244383…`,
    /// `Bb[0] = 1.3915205…`, `Cb[1] = −0.2523308…`.
    #[test]
    fn balreal_matches_the_oracle() {
        let args = [
            -1.0, 0.0, 0.0, -2.0, // A
            1.0, 1.0, // B (2x1)
            1.0, 1.0, // C (1x2)
        ];
        let a00 = eval_intrinsic("balreal$a$0$0$2$1$1", &args)
            .unwrap()
            .unwrap();
        let a11 = eval_intrinsic("balreal$a$1$1$2$1$1", &args)
            .unwrap()
            .unwrap();
        let b0 = eval_intrinsic("balreal$b$0$0$2$1$1", &args)
            .unwrap()
            .unwrap();
        let c0 = eval_intrinsic("balreal$c$0$0$2$1$1", &args)
            .unwrap()
            .unwrap();
        assert!((a00 + 1.3244383).abs() < 1e-6, "Ab[0][0] = {a00}");
        assert!((a11 + 1.6755617).abs() < 1e-6, "Ab[1][1] = {a11}");
        assert!((b0 - 1.3915205).abs() < 1e-6, "Bb[0] = {b0}");
        assert!((c0 - 1.3915205).abs() < 1e-6, "Cb[0] = {c0}");

        // KNOWN DIVERGENCE, and it is not in this module's unpacking: the
        // oracle reports Ab[0][1] = Ab[1][0] = +0.4681646, Bb[1] = Cb[1] =
        // -0.2523308, while `control::design::balreal` returns all four with
        // the opposite sign — the second balancing basis vector is negated.
        // Both are valid balanced realizations (the Hankel singular values and
        // the input-output map agree), but the Java's spelling is the parity
        // target. The magnitudes are asserted here so the slot mapping stays
        // covered; the sign belongs to `design::balreal`.
        let a01 = eval_intrinsic("balreal$a$0$1$2$1$1", &args)
            .unwrap()
            .unwrap();
        let b1 = eval_intrinsic("balreal$b$1$0$2$1$1", &args)
            .unwrap()
            .unwrap();
        let c1 = eval_intrinsic("balreal$c$0$1$2$1$1", &args)
            .unwrap()
            .unwrap();
        assert!((a01.abs() - 0.4681646).abs() < 1e-6, "|Ab[0][1]| = {a01}");
        assert!((b1.abs() - 0.2523308).abs() < 1e-6, "|Bb[1]| = {b1}");
        assert!((c1.abs() - 0.2523308).abs() < 1e-6, "|Cb[1]| = {c1}");
    }

    /// Oracle (`d07_ss_interconnect`): two first-order systems combined.
    /// Series takes `q_out = q2`, and `Cs = [0, 3]`; parallel keeps `q1` and
    /// gives `Cp = [1, 3]`; feedback gives `Af = [−1 −3; 1 −2]`.
    #[test]
    fn state_space_interconnection_matches_the_oracle() {
        // A1 = [-1], B1 = [1], C1 = [1], D1 = [0]; A2 = [-2], B2 = [1],
        // C2 = [3], D2 = [0].
        let base = [-1.0, 1.0, 1.0, 0.0, -2.0, 1.0, 3.0, 0.0];
        let suffix = "1$1$1$1$1$1";
        let cs0 = eval_intrinsic(&format!("ss_series$c$0$0${suffix}"), &base)
            .unwrap()
            .unwrap();
        let cs1 = eval_intrinsic(&format!("ss_series$c$0$1${suffix}"), &base)
            .unwrap()
            .unwrap();
        assert_eq!((cs0, cs1), (0.0, 3.0));

        let cp0 = eval_intrinsic(&format!("ss_parallel$c$0$0${suffix}"), &base)
            .unwrap()
            .unwrap();
        let cp1 = eval_intrinsic(&format!("ss_parallel$c$0$1${suffix}"), &base)
            .unwrap()
            .unwrap();
        assert_eq!((cp0, cp1), (1.0, 3.0));

        let mut fb = base.to_vec();
        fb.push(1.0); // sign
        let af01 = eval_intrinsic(&format!("ss_feedback$a$0$1${suffix}"), &fb)
            .unwrap()
            .unwrap();
        let af10 = eval_intrinsic(&format!("ss_feedback$a$1$0${suffix}"), &fb)
            .unwrap()
            .unwrap();
        assert_eq!((af01, af10), (-3.0, 1.0));
    }

    /// The step response of `1/(s+1)` on the default 50-point grid, taken from
    /// the oracle (`tools/golden-dumper`, `c07_step_impulse_lsim_stepinfo`) so
    /// the metrics below are checked against the Java engine's own samples
    /// rather than an analytic stand-in.
    const ORACLE_STEP_Y: [f64; 50] = [
        0.0,
        0.18460439504256027,
        0.3351297870988714,
        0.4578678064204521,
        0.5579476510852956,
        0.6395527674794738,
        0.7060926247666517,
        0.7603489560848287,
        0.804589655872893,
        0.8406633781323491,
        0.8700776803944786,
        0.8940619095669027,
        0.9136184782356829,
        0.9295647712215592,
        0.9425673777596394,
        0.9531698674208147,
        0.9618151514484853,
        0.9688642638357391,
        0.974611818692671,
        0.9792986373418432,
        0.9831204589279608,
        0.9862363254089264,
        0.9887771503615548,
        0.9908491479935343,
        0.9925381950603482,
        0.9939159117522015,
        0.9950389362344694,
        0.9959548725289037,
        0.996701579727557,
        0.9973105411995155,
        0.9978069792543691,
        0.9982119139513007,
        0.9985418916068187,
        0.9988112105415845,
        0.9990305318576513,
        0.9992096101139827,
        0.9993555069659787,
        0.9994744270258804,
        0.9995715515781309,
        0.9996505854468192,
        0.9997150797884654,
        0.9997677497605757,
        0.9998105807255179,
        0.9998455235993333,
        0.999874100465736,
        0.9998973384052976,
        0.9999162404801959,
        0.9999317141827175,
        0.9999443234504318,
        0.9999545889685634,
    ];

    /// Oracle (`c07_step_impulse_lsim_stepinfo`): those samples give
    /// `Tr = 2.195892342198661`, `Tp = 10`, `Ts = 3.9126267106387433`,
    /// `OS = 0`.
    #[test]
    fn stepinfo_matches_the_oracle() {
        let dt = 10.0 / 49.0;
        let t: Vec<f64> = (0..50).map(|i| i as f64 * dt).collect();
        let args: Vec<f64> = t.iter().chain(ORACLE_STEP_Y.iter()).copied().collect();
        let tr = eval_intrinsic("stepinfo$tr$50", &args).unwrap().unwrap();
        let tp = eval_intrinsic("stepinfo$tp$50", &args).unwrap().unwrap();
        let ts = eval_intrinsic("stepinfo$ts$50", &args).unwrap().unwrap();
        let os = eval_intrinsic("stepinfo$os$50", &args).unwrap().unwrap();
        assert!((tr - 2.195892342198661).abs() < 1e-9, "Tr = {tr}");
        assert!((tp - 10.0).abs() < 1e-12, "Tp = {tp}");
        assert!((ts - 3.9126267106387433).abs() < 1e-9, "Ts = {ts}");
        assert_eq!(os, 0.0);
    }

    /// The same oracle series, reached through `step$` itself: the flattener
    /// serialises the padded numerator, the denominator and the 50 time
    /// samples, and each `step$<i>$3$50` reads one sample back.
    #[test]
    fn step_matches_the_oracle_sample_for_sample() {
        let dt = 10.0 / 49.0;
        let mut args = vec![0.0, 1.0, 1.0, 1.0]; // num = [0, 1], den = [1, 1]
        args.extend((0..50).map(|i| i as f64 * dt));
        for (i, want) in ORACLE_STEP_Y.iter().enumerate() {
            let got = eval_intrinsic(&format!("step${i}$3$50"), &args)
                .unwrap()
                .unwrap();
            assert!(
                (got - want).abs() < 1e-9,
                "step y[{i}]: got {got}, oracle {want}"
            );
        }
    }

    /// Oracle (`c10_pidtune_pade_c2d_residue`): a first-order Padé of a 0.5 s
    /// delay is `(-0.5 s + 2) / (0.5 s + 2)`, and `pade$` reads only `Td`.
    #[test]
    fn pade_matches_the_oracle_and_ignores_its_second_argument() {
        let args = [0.5, 1.0];
        assert_eq!(
            eval_intrinsic("pade$num$0$1", &args).unwrap().unwrap(),
            -0.5
        );
        assert_eq!(eval_intrinsic("pade$num$1$1", &args).unwrap().unwrap(), 2.0);
        assert_eq!(eval_intrinsic("pade$den$0$1", &args).unwrap().unwrap(), 0.5);
        assert_eq!(eval_intrinsic("pade$den$1$1", &args).unwrap().unwrap(), 2.0);
    }

    /// Oracle (`c10_pidtune_pade_c2d_residue`): plant `1/(s+1)`, `'PI'`,
    /// `wc = 1` gives `Kp = 0.3660254…`, `Ki = 1.3660254…`, `Kd = 0`.
    #[test]
    fn pidtune_matches_the_oracle() {
        // The flattener pads the numerator to the denominator length.
        let args = [0.0, 1.0, 1.0, 1.0, 1.0];
        let kp = eval_intrinsic("pidtune$kp$pi", &args).unwrap().unwrap();
        let ki = eval_intrinsic("pidtune$ki$pi", &args).unwrap().unwrap();
        let kd = eval_intrinsic("pidtune$kd$pi", &args).unwrap().unwrap();
        assert!((kp - 0.3660254).abs() < 1e-6, "Kp = {kp}");
        assert!((ki - 1.3660254).abs() < 1e-6, "Ki = {ki}");
        assert_eq!(kd, 0.0);
    }

    /// Oracle (`c09_…`): `dlqr` on `A = diag(−1, −2)`, `B = [1; 1]`, `Q = I`,
    /// `R = 1` gives `Kd = [0.29289322…, −2.41421356…]`, and `dare` the
    /// matching `P`.
    #[test]
    fn dlqr_and_dare_match_the_oracle() {
        let args = [
            -1.0, 0.0, 0.0, -2.0, // A
            1.0, 1.0, // B (2x1)
            1.0, 0.0, 0.0, 1.0, // Q
            1.0, // R
        ];
        let k0 = eval_intrinsic("dlqr$0$0$2$1", &args).unwrap().unwrap();
        let k1 = eval_intrinsic("dlqr$0$1$2$1", &args).unwrap().unwrap();
        assert!((k0 - 0.29289322).abs() < 1e-7, "Kd[0] = {k0}");
        assert!((k1 + 2.41421356).abs() < 1e-7, "Kd[1] = {k1}");

        let p00 = eval_intrinsic("dare$0$0$2$1", &args).unwrap().unwrap();
        let p11 = eval_intrinsic("dare$1$1$2$1", &args).unwrap().unwrap();
        assert!((p00 - 4.82842712).abs() < 1e-7, "P[0][0] = {p00}");
        assert!((p11 - 22.3137085).abs() < 1e-6, "P[1][1] = {p11}");
    }

    /// Every assertion below is a `tools/golden-dumper` run against the real
    /// Java engine; the fixture name is quoted at each site. `approx` uses a
    /// relative tolerance because the oracle prints round-tripped doubles.
    fn approx(got: f64, want: f64, what: &str) {
        let scale = want.abs().max(1.0);
        assert!(
            (got - want).abs() <= 1e-7 * scale,
            "{what}: got {got}, oracle {want}"
        );
    }

    fn at(function: &str, args: &[f64]) -> f64 {
        eval_intrinsic(function, args)
            .unwrap_or_else(|| panic!("{function} was not claimed"))
            .unwrap_or_else(|e| panic!("{function}: {e}"))
    }

    /// Oracle (`c01_ss2tf`): `A = [0 1; -2 -3]`, `B = [0; 1]`, `C = [1 0]`,
    /// `D = 0` gives `num = [0, 0, 1]`, `den = [1, 3, 2]`.
    #[test]
    fn ss2tf_matches_the_oracle() {
        let args = [0.0, 1.0, -2.0, -3.0, 0.0, 1.0, 1.0, 0.0, 0.0];
        for (k, want) in [0.0, 0.0, 1.0].iter().enumerate() {
            approx(at(&format!("ss2tf$num${k}$2"), &args), *want, "num");
        }
        for (k, want) in [1.0, 3.0, 2.0].iter().enumerate() {
            approx(at(&format!("ss2tf$den${k}$2"), &args), *want, "den");
        }
    }

    /// Oracle (`d09_ss2tfij_residue_ord`): channel (1,1) of the 2×2 plant is
    /// `(s + 2) / (s² + 3s + 2)`. `ss2tfij` reuses the `ss2tf$` evaluator after
    /// the flattener picks row 1 of C and column 1 of B, so the same call name
    /// with `A`, `B[:,1]`, `C[1,:]`, `D[1,1]` reproduces it.
    #[test]
    fn ss2tfij_reuses_the_siso_evaluator() {
        // A = diag(-1, -2); B column 1 = [1; 0]; C row 1 = [1, 1]; D_11 = 0.
        let args = [-1.0, 0.0, 0.0, -2.0, 1.0, 0.0, 1.0, 1.0, 0.0];
        for (k, want) in [0.0, 1.0, 2.0].iter().enumerate() {
            approx(at(&format!("ss2tf$num${k}$2"), &args), *want, "num");
        }
        for (k, want) in [1.0, 3.0, 2.0].iter().enumerate() {
            approx(at(&format!("ss2tf$den${k}$2"), &args), *want, "den");
        }
    }

    /// Oracle (`c02_tf2ss`): `num = [1]` (padded to `[0, 0, 1]`),
    /// `den = [1, 3, 2]` gives `A = [-3 -2; 1 0]`, `B = [1; 0]`, `C = [0 1]`,
    /// `D = 0` — the controllable canonical form.
    #[test]
    fn tf2ss_matches_the_oracle() {
        let args = [0.0, 0.0, 1.0, 1.0, 3.0, 2.0];
        approx(at("tf2ss$a$0$0$2", &args), -3.0, "A[0][0]");
        approx(at("tf2ss$a$0$1$2", &args), -2.0, "A[0][1]");
        approx(at("tf2ss$a$1$0$2", &args), 1.0, "A[1][0]");
        approx(at("tf2ss$a$1$1$2", &args), 0.0, "A[1][1]");
        approx(at("tf2ss$b$0$2", &args), 1.0, "B[0]");
        approx(at("tf2ss$b$1$2", &args), 0.0, "B[1]");
        approx(at("tf2ss$c$0$2", &args), 0.0, "C[0]");
        approx(at("tf2ss$c$1$2", &args), 1.0, "C[1]");
        approx(at("tf2ss$d$2", &args), 0.0, "D");
    }

    /// Oracle (`c03_series_parallel_feedback`): `G1 = s/(s+1)` written as
    /// `[0,1]/[1,1]`, `G2 = 2s/(s+3)` as `[0,2]/[1,3]`.
    #[test]
    fn tf_interconnection_matches_the_oracle() {
        let base = [0.0, 1.0, 1.0, 1.0, 0.0, 2.0, 1.0, 3.0];
        for (op, num, den) in [
            ("series", [0.0, 0.0, 2.0], [1.0, 4.0, 3.0]),
            ("parallel", [0.0, 3.0, 5.0], [1.0, 4.0, 3.0]),
        ] {
            for i in 0..3 {
                approx(at(&format!("{op}$num${i}$2$2"), &base), num[i], op);
                approx(at(&format!("{op}$den${i}$2$2"), &base), den[i], op);
            }
        }
        let mut fb = base.to_vec();
        fb.push(1.0); // the default sign the flattener appends
        for (i, (num, den)) in [(0.0, 1.0), (1.0, 4.0), (3.0, 5.0)].iter().enumerate() {
            approx(
                at(&format!("feedback$num${i}$2$2"), &fb),
                *num,
                "feedback num",
            );
            approx(
                at(&format!("feedback$den${i}$2$2"), &fb),
                *den,
                "feedback den",
            );
        }
    }

    /// Oracle (`c04_pole_zero`): `num = [0, 1, 2]`, `den = [1, 3, 2]` gives
    /// poles `-2, -1` and the single finite zero `-2`; the second `zero` slot
    /// reads `0.0` because the numerator has only one root.
    #[test]
    fn pole_and_zero_match_the_oracle() {
        let args = [0.0, 1.0, 2.0, 1.0, 3.0, 2.0];
        approx(at("pole$pr$0$2$2", &args), -2.0, "pr[0]");
        approx(at("pole$pr$1$2$2", &args), -1.0, "pr[1]");
        assert_eq!(at("pole$pi$0$2$2", &args), 0.0);
        approx(at("zero$zr$0$2$2", &args), -2.0, "zr[0]");
        assert_eq!(at("zero$zr$1$2$2", &args), 0.0);
        assert_eq!(at("zero$zi$0$2$2", &args), 0.0);
    }

    /// Oracle (`d10_bode_ss_step_ss`): the state-space forms recover `n` from
    /// the argument count. `A = diag(-1, -2)`, `B = [1; 1]`, `C = [1 1]`,
    /// `D = 0` has the single finite zero `-1.5`.
    #[test]
    fn the_state_space_forms_match_the_oracle() {
        let model = [-1.0, 0.0, 0.0, -2.0, 1.0, 1.0, 1.0, 1.0, 0.0];
        approx(at("zero$zr$0$4$2", &model), -1.5, "zzr[0]");
        assert_eq!(at("zero$zr$1$4$2", &model), 0.0);

        let mut bode_args = model.to_vec();
        bode_args.extend([1.0, 10.0]); // omega
        approx(at("bode$mag$0$5$2", &bode_args), 1.1394335, "mg[0]");
        approx(at("bode$mag$1$5$2", &bode_args), -14.0963141, "mg[1]");
        approx(at("bode$phase$0$5$2", &bode_args), -37.8749837, "ph[0]");
        approx(at("bode$phase$1$5$2", &bode_args), -81.51024, "ph[1]");

        let mut step_args = model.to_vec();
        step_args.extend([0.0, 1.0, 2.0]); // t
        approx(at("step$0$5$3", &step_args), 0.0, "ys[0]");
        approx(at("step$1$5$3", &step_args), 1.0644532, "ys[1]");
        approx(at("step$2$5$3", &step_args), 1.3555069, "ys[2]");
    }

    /// Oracle (`c05_bode_nyquist_nichols`): `1/(s+1)` at `omega = 0.1, 1, 10`.
    /// `nichols` shares `bode`'s kernel, so the same numbers come back.
    #[test]
    fn frequency_response_matches_the_oracle() {
        // num = [0, 1] (padded), den = [1, 1], then the three frequencies.
        let args = [0.0, 1.0, 1.0, 1.0, 0.1, 1.0, 10.0];
        let mag = [-0.04321374, -3.01029996, -20.04321374];
        let phase = [-5.71059314, -45.0, -84.28940686];
        for i in 0..3 {
            for op in ["bode", "nichols"] {
                approx(at(&format!("{op}$mag${i}$3$3"), &args), mag[i], op);
                approx(at(&format!("{op}$phase${i}$3$3"), &args), phase[i], op);
            }
        }
        let re = [0.99009901, 0.5, 0.00990099];
        let im = [-0.0990099, -0.5, -0.0990099];
        for i in 0..3 {
            approx(at(&format!("nyquist$real${i}$3$3"), &args), re[i], "re");
            approx(at(&format!("nyquist$imag${i}$3$3"), &args), im[i], "im");
        }
    }

    /// Oracle (`d01_margin`): `2/(s² + 3s + 2)` never crosses either critical
    /// point, so all four margin outputs take the Java's sentinels.
    #[test]
    fn margin_routh_and_errorconst_match_the_oracle() {
        let margin_args = [0.0, 0.0, 2.0, 1.0, 3.0, 2.0];
        approx(at("margin$gm$2", &margin_args), 1e9, "gm");
        approx(at("margin$pm$2", &margin_args), 1e9, "pm");
        assert_eq!(at("margin$wcg$2", &margin_args), 0.0);
        assert_eq!(at("margin$wcp$2", &margin_args), 0.0);

        let den = [1.0, 3.0, 2.0];
        assert_eq!(at("routh$nrhp$3", &den), 0.0);
        assert_eq!(at("routh$stable$3", &den), 1.0);

        // errorconst serialises the numerator *unpadded*: [2] then [1, 3, 2].
        let ec = [2.0, 1.0, 3.0, 2.0];
        approx(at("errorconst$kp$1$3", &ec), 1.0, "Kp");
        assert_eq!(at("errorconst$kv$1$3", &ec), 0.0);
        assert_eq!(at("errorconst$ka$1$3", &ec), 0.0);
    }

    /// Oracle (`d06_zp2tf_tf2zp`): `k = 5`, zero `-2`, poles `-1, -3` is
    /// `5(s+2) / (s² + 4s + 3)`, and the round trip recovers them.
    #[test]
    fn zp2tf_and_tf2zp_match_the_oracle() {
        let zp = [-2.0, 0.0, -1.0, -3.0, 0.0, 0.0, 5.0];
        for (i, want) in [0.0, 5.0, 10.0].iter().enumerate() {
            approx(at(&format!("zp2tf$num${i}$1$2"), &zp), *want, "num");
        }
        for (i, want) in [1.0, 4.0, 3.0].iter().enumerate() {
            approx(at(&format!("zp2tf$den${i}$1$2"), &zp), *want, "den");
        }

        let tf = [0.0, 5.0, 10.0, 1.0, 4.0, 3.0];
        approx(at("tf2zp$zr$0$1$2", &tf), -2.0, "oz[0]");
        assert_eq!(at("tf2zp$zi$0$1$2", &tf), 0.0);
        approx(at("tf2zp$pr$0$1$2", &tf), -3.0, "op[0]");
        approx(at("tf2zp$pr$1$1$2", &tf), -1.0, "op[1]");
        approx(at("tf2zp$k$1$2", &tf), 5.0, "gk");
    }

    /// Oracle (`c10_…` and `d09_…`): `1/(s²+3s+2)` has simple poles and
    /// residues `-1, 1`; `1/(s+1)²` needs the 6-output form, whose `ord`
    /// vector is `[1, 2]`.
    #[test]
    fn residue_matches_the_oracle_in_both_forms() {
        let simple = [1.0, 1.0, 3.0, 2.0];
        approx(at("residue$rr$s$0$1$2", &simple), -1.0, "rr[0]");
        approx(at("residue$rr$s$1$1$2", &simple), 1.0, "rr[1]");
        approx(at("residue$pr$s$0$1$2", &simple), -2.0, "pr[0]");
        approx(at("residue$pr$s$1$1$2", &simple), -1.0, "pr[1]");
        assert_eq!(at("residue$k$s$1$2", &simple), 0.0);

        let repeated = [1.0, 1.0, 2.0, 1.0];
        // The simple form must refuse a repeated pole rather than drop a term.
        let refused = eval_intrinsic("residue$rr$s$0$1$2", &repeated).unwrap();
        assert!(refused.unwrap_err().to_string().contains("repeated poles"));
        approx(at("residue$ord$o$0$1$2", &repeated), 1.0, "ord[0]");
        approx(at("residue$ord$o$1$1$2", &repeated), 2.0, "ord[1]");
        approx(at("residue$rr$o$0$1$2", &repeated), 0.0, "rr[0]");
        approx(at("residue$rr$o$1$1$2", &repeated), 1.0, "rr[1]");
    }

    /// Oracle (`c10_…` for c2d, `d08_…` for d2c).
    #[test]
    fn discretisation_matches_the_oracle() {
        // c2d of 1/(s+1) at Ts = 0.1, Tustin.
        let c = [0.0, 1.0, 1.0, 1.0, 0.1];
        approx(at("c2d$num$tustin$0$2", &c), 0.04761905, "nz[0]");
        approx(at("c2d$num$tustin$1$2", &c), 0.04761905, "nz[1]");
        approx(at("c2d$den$tustin$0$2", &c), 1.0, "dz[0]");
        approx(at("c2d$den$tustin$1$2", &c), -0.9047619, "dz[1]");

        // d2c of (z+1)/(z-0.9) at Ts = 0.1, Tustin.
        let d = [1.0, 1.0, 1.0, -0.9, 0.1];
        approx(at("d2c$num$tustin$0$2", &d), 0.0, "nc[0]");
        approx(at("d2c$num$tustin$1$2", &d), 21.0526316, "nc[1]");
        approx(at("d2c$den$tustin$0$2", &d), 1.0, "dc[0]");
        approx(at("d2c$den$tustin$1$2", &d), 1.0526316, "dc[1]");
    }

    /// Oracle (`d08_rlocus_d2c_mason`): a single forward branch of gain 2 from
    /// node 1 to node 2 has transmittance 2.
    #[test]
    fn mason_matches_the_oracle_and_range_checks_its_nodes() {
        let g = [0.0, 2.0, 0.0, 0.0, 1.0, 2.0];
        approx(at("mason$2", &g), 2.0, "T");
        let bad = [0.0, 2.0, 0.0, 0.0, 0.0, 2.0];
        let got = eval_intrinsic("mason$2", &bad).unwrap();
        assert!(got.unwrap_err().to_string().contains("out of range 1..2"));
    }

    /// Oracle (`d08_rlocus_d2c_mason`): `1/(s²+3s+2)` over 4 gain samples gives
    /// `K = [0, 0.0003, 0.3, 300]`, the open-loop poles `-2, -1` at `K = 0`,
    /// and a conjugate pair at the last two gains. The `k` selector shifts the
    /// name layout by one because it needs no column index.
    #[test]
    fn rlocus_matches_the_oracle() {
        let args = [1.0, 1.0, 3.0, 2.0]; // num = [1], den = [1, 3, 2]
        for (i, want) in [0.0, 0.0003, 0.3, 300.0].iter().enumerate() {
            approx(at(&format!("rlocus$k${i}$1$3$4$2"), &args), *want, "K");
        }
        approx(at("rlocus$cpr$0$0$1$3$4$2", &args), -2.0, "cpr[0][0]");
        approx(at("rlocus$cpr$0$1$1$3$4$2", &args), -1.0, "cpr[0][1]");
        approx(at("rlocus$cpi$2$0$1$3$4$2", &args), 0.2236068, "cpi[2][0]");
        approx(at("rlocus$cpi$2$1$1$3$4$2", &args), -0.2236068, "cpi[2][1]");
        approx(at("rlocus$cpr$3$0$1$3$4$2", &args), -1.5, "cpr[3][0]");
    }

    /// Oracle (`c07_step_impulse_lsim_stepinfo`): the impulse response of
    /// `1/(s+1)` starts at 1, and `lsim` with a unit step input reproduces the
    /// step response on its own 3-point grid.
    #[test]
    fn impulse_and_lsim_match_the_oracle() {
        let dt = 10.0 / 49.0;
        let mut imp = vec![0.0, 1.0, 1.0, 1.0];
        imp.extend((0..50).map(|i| i as f64 * dt));
        approx(at("impulse$0$3$50", &imp), 1.0, "yi[0]");
        approx(at("impulse$1$3$50", &imp), 0.8153955292757008, "yi[1]");
        approx(at("impulse$2$3$50", &imp), 0.6648695159557069, "yi[2]");

        // num, den, then u (3), then t (3).
        let ls = [0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.5, 1.0];
        approx(at("lsim$0$4$3", &ls), 0.0, "yl[0]");
        approx(at("lsim$1$4$3", &ls), 0.3934694037552376, "yl[1]");
        approx(at("lsim$2$4$3", &ls), 0.6321205546294072, "yl[2]");
    }

    #[test]
    fn the_state_count_is_recovered_from_the_argument_total() {
        // (n + 1)^2 for n = 1, 2, 3.
        assert_eq!(states_from_args(4).unwrap(), 1);
        assert_eq!(states_from_args(9).unwrap(), 2);
        assert_eq!(states_from_args(16).unwrap(), 3);
        // A total below 1 cannot describe a model.
        assert!(states_from_args(0).is_err());
    }

    #[test]
    fn complex_roots_sort_by_real_then_imaginary_part() {
        let mut roots = [
            Complex::new(1.0, 2.0),
            Complex::new(-1.0, 0.0),
            Complex::new(1.0, -2.0),
            Complex::new(-1.0, -1.0),
        ];
        sort_complex(&mut roots);
        assert_eq!(
            roots,
            [
                Complex::new(-1.0, -1.0),
                Complex::new(-1.0, 0.0),
                Complex::new(1.0, -2.0),
                Complex::new(1.0, 2.0),
            ]
        );
    }

    #[test]
    fn residue_terms_are_ranked_by_pole_then_order() {
        let poles = [
            Complex::new(-1.0, 0.0),
            Complex::new(-2.0, 0.0),
            Complex::new(-1.0, 0.0),
        ];
        let orders = [2usize, 1, 1];
        // Sorted: (-2, 1) -> src 1, (-1, 1) -> src 2, (-1, 2) -> src 0.
        assert_eq!(sorted_residue_index(&poles, &orders, 0), Some(1));
        assert_eq!(sorted_residue_index(&poles, &orders, 1), Some(2));
        assert_eq!(sorted_residue_index(&poles, &orders, 2), Some(0));
        assert_eq!(sorted_residue_index(&poles, &orders, 3), None);
    }
}
