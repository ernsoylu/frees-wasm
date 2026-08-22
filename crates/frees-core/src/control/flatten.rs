//! Control-systems `CALL` flattening.
//!
//! Port of `parser/ControlSystemsFlattener.java` (1,978 LOC) — the half of the
//! CALL surface that turns a control-systems statement into scalar equations
//! whose right-hand sides are `$`-synthetics, exactly as Phase 4 did for the
//! linear-algebra / signal / statistics CALLs (`qr$…`, `fft$…`, `svd$…`).
//!
//! # Where this runs
//!
//! [`crate::parser::expand`] owns the matrix-expansion pass and its
//! `MatrixInfo`/`VectorInfo` slice resolution. Rather than duplicate that (or
//! move it), this module states the four read-only queries and the three
//! writes it needs as the [`Shapes`] / [`Host`] traits, and `expand.rs`
//! supplies a thin adapter over its own `Flattener`. The Java reaches the same
//! five `EquationParser` helpers through a back-reference field
//! (`csFlattener`); the trait pair is that back-reference, typed.
//!
//! # The two-stage shape
//!
//! 1. [`auto_size`] is the control-systems half of `autoSizeCallOutputs`:
//!    bare output names (`CALL step(num, den : y)`) grow into explicit
//!    `y[1:N]` slices sized from the inputs. Failures are swallowed by the
//!    caller so the flattener's own, more specific error wins — the Java's
//!    `catch (ParseException ignored)`.
//! 2. [`flatten`] dispatches to one handler per intrinsic, each emitting
//!    `out[i] = <name>$…(entries…)` equations. The serialised `entries` layout
//!    is **load-bearing**: [`crate::control::eval`] reconstructs the model from
//!    exactly that order.
//!
//! # Behaviour ported bug-for-bug
//!
//! * `getVectorElements` / `getRowVectorElements` / `getScalarElement` name
//!   `ss2tf` in their error text no matter which intrinsic called them.
//! * `getMatrixData` reads a bare `Expr::Var` as a 1×1 matrix holding the
//!   *unexpanded* variable, and a numeric literal as a 1×1 matrix named
//!   `scalar`.
//! * `flattenSsCombine`'s `p_out`/`q_out` are computed with a `Math.max` arm
//!   and a `feedback` arm that the immediately following `if` overwrites and
//!   that `flattenSsCombine` can never reach; only the surviving values
//!   (series → `(p1, q2)`, parallel → `(p1, q1)`) are transcribed.
//! * `flattenPade` serialises its two inputs **raw**, without expanding them.
//! * `padNumerator` left-pads a short numerator with zeros; a numerator longer
//!   than the denominator is an improper transfer function and is refused.
//!
//! # One deliberate divergence
//!
//! Where the Java indexes a user-declared output matrix that is smaller than
//! the shape the intrinsic produces, it throws `IndexOutOfBoundsException` —
//! an unchecked exception that escapes the parser as an HTTP 500. A wasm
//! engine cannot panic on user input, so every such site here checks first and
//! returns a parse error naming the required shape. See
//! [`require_output_cells`].

use crate::ast::{Equation, Expr};
use crate::diag::{FreesError, Result};

/// Default step/impulse response horizon (seconds) when the caller omits an
/// explicit time vector. Port of
/// `ControlSystemsFlattener.DEFAULT_TIME_FINAL`.
pub const DEFAULT_TIME_FINAL: f64 = 10.0;

/// Sample count of that default grid. Port of
/// `ControlSystemsFlattener.DEFAULT_TIME_POINTS`.
pub const DEFAULT_TIME_POINTS: usize = 50;

/// Root-locus gain samples an auto-sized `rlocus` output gets. Port of
/// `EquationParser.DEFAULT_RLOCUS_POINTS`.
pub const DEFAULT_RLOCUS_POINTS: usize = 100;

/// The CALL names this module flattens, lowercase. `expand::flatten_call_proc`
/// dispatches on membership, and `procedures::EXPANDED_CALL_TARGETS` must list
/// the same names so a stage-2 pass lets them through untouched.
pub const CALL_NAMES: [&str; 40] = [
    "ss2tf",
    "ss2tfij",
    "tf2ss",
    "zp2tf",
    "tf2zp",
    "series",
    "parallel",
    "feedback",
    "pole",
    "zero",
    "bode",
    "nyquist",
    "margin",
    "step",
    "impulse",
    "lsim",
    "lqr",
    "dlqr",
    "dare",
    "lyap",
    "dlyap",
    "place",
    "acker",
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
    "routh",
    "c2d",
    "d2c",
    "residue",
    "nichols",
    "errorconst",
];

/// True when [`flatten`] handles `name` (already lowercased).
pub fn handles(name: &str) -> bool {
    CALL_NAMES.contains(&name) || name == "mason"
}

// ---------------------------------------------------------------------------
// The host interface
// ---------------------------------------------------------------------------

/// An `A[r1:r2, c1:c2]` reference resolved to its element variables. The Rust
/// `EquationParser.MatrixInfo` (minus `rowStart`/`colStart`, which no
/// control-systems handler reads).
#[derive(Debug, Clone)]
pub struct MatrixRef {
    /// Base name of the sliced variable, lowercase (`""` for a literal).
    pub name: String,
    pub rows: usize,
    pub cols: usize,
    /// Row-major element variables, `rows` × `cols`.
    pub elements: Vec<Vec<Expr>>,
}

/// A `v[a:b]` reference resolved to its element variables. The Rust
/// `EquationParser.VectorInfo`.
#[derive(Debug, Clone)]
pub struct VectorRef {
    /// Base name of the sliced variable, lowercase.
    pub name: String,
    pub size: usize,
    pub elements: Vec<Expr>,
}

impl MatrixRef {
    /// Row-major flattening, the order every serialised `entries` list uses.
    fn entries(&self) -> Vec<Expr> {
        let mut out = Vec::with_capacity(self.rows.saturating_mul(self.cols));
        for row in &self.elements {
            out.extend(row.iter().cloned());
        }
        out
    }

    /// The output slots in the order the Java's `outFlattened` list builds
    /// them: every declared cell, row-major, regardless of the shape the
    /// intrinsic is about to write.
    fn out_slots(&self) -> Vec<Expr> {
        self.entries()
    }
}

/// The read-only slice queries the control flatteners need from the
/// matrix-expansion pass. Port of the `parseMatrixInfo` / `parseVectorInfo` /
/// `expandExpr` / `constIndex` back-references in
/// `ControlSystemsFlattener(EquationParser)`.
pub trait Shapes {
    /// `EquationParser.parseMatrixInfo`.
    fn matrix_info(&self, expr: &Expr) -> Result<MatrixRef>;
    /// `EquationParser.parseVectorInfo`.
    fn vector_info(&self, expr: &Expr) -> Result<VectorRef>;
    /// `EquationParser.expandExpr`.
    fn expand(&self, expr: &Expr) -> Result<Expr>;
    /// `EquationParser.constIndex` — a compile-time integer.
    fn const_index(&self, expr: &Expr) -> Result<i64>;
}

/// The writes on top of [`Shapes`]: shape registration and equation emission.
pub trait Host: Shapes {
    /// `EquationParser.registerShape` — records an output's shape so later
    /// statements can name it bare.
    fn register_shape(&mut self, name: &str, rows: usize, cols: usize);
    /// Append one generated equation (the Java `ctx.out().add`), enforcing the
    /// generated-equation budget.
    fn emit(&mut self, equation: Equation) -> Result<()>;
    /// Assert budget headroom before a batch is built, so an oversized request
    /// cannot allocate O(equations × entries) first. Mirrors
    /// `expand::Flattener::reserve`.
    fn reserve(&self, planned: usize) -> Result<()>;
}

fn parse_err(message: impl Into<String>) -> FreesError {
    FreesError::parse(message)
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Flatten one control-systems `CALL`. Port of the `csFlattener.…` chain in
/// `EquationParser.flattenCallProc`, including its two arity splits:
/// `series`/`parallel` take the state-space form at 8 inputs, `feedback` at 8
/// or 9, and `acker` is an alias of `place`.
pub fn flatten<H: Host + ?Sized>(
    host: &mut H,
    name: &str,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    match name {
        "ss2tf" => flatten_ss2tf(host, inputs, outputs, source),
        "ss2tfij" => flatten_ss2tf_mimo(host, inputs, outputs, source),
        "tf2ss" => flatten_tf2ss(host, inputs, outputs, source),
        "zp2tf" => flatten_zp2tf(host, inputs, outputs, source),
        "tf2zp" => flatten_tf2zp(host, inputs, outputs, source),
        "series" | "parallel" if inputs.len() == 8 => {
            flatten_ss_combine(host, name, inputs, outputs, source)
        }
        "series" | "parallel" => flatten_tf_combine(host, name, inputs, outputs, source),
        "feedback" if inputs.len() == 8 || inputs.len() == 9 => {
            flatten_ss_feedback(host, inputs, outputs, source)
        }
        "feedback" => flatten_feedback(host, inputs, outputs, source),
        "pole" => flatten_pole(host, inputs, outputs, source),
        "zero" => flatten_zero(host, inputs, outputs, source),
        "bode" => flatten_freq_response(host, "bode", "mag", "phase", inputs, outputs, source),
        "nyquist" => {
            flatten_freq_response(host, "nyquist", "real", "imag", inputs, outputs, source)
        }
        "nichols" => {
            flatten_freq_response(host, "nichols", "mag", "phase", inputs, outputs, source)
        }
        "margin" => flatten_margin(host, inputs, outputs, source),
        "step" | "impulse" => flatten_time_response(host, name, inputs, outputs, source),
        "lsim" => flatten_lsim(host, inputs, outputs, source),
        "lqr" | "dlqr" | "dare" => flatten_lqr_like(host, name, inputs, outputs, source),
        "lyap" | "dlyap" => flatten_lyap_like(host, name, inputs, outputs, source),
        "place" | "acker" => flatten_place(host, inputs, outputs, source),
        "lqe" => flatten_lqe(host, inputs, outputs, source),
        "gram" => flatten_gram(host, inputs, outputs, source),
        "balreal" => flatten_balreal(host, inputs, outputs, source),
        "pidtune" => flatten_pidtune(host, inputs, outputs, source),
        "rank" => flatten_rank(host, inputs, outputs, source),
        "ctrb" | "obsv" => flatten_ctrb_obsv(host, name, inputs, outputs, source),
        "ss2ss" => flatten_ss2ss(host, inputs, outputs, source),
        "stepinfo" => flatten_stepinfo(host, inputs, outputs, source),
        "pade" => flatten_pade(host, inputs, outputs, source),
        "rlocus" => flatten_rlocus(host, inputs, outputs, source),
        "routh" => flatten_routh(host, inputs, outputs, source),
        "c2d" | "d2c" => flatten_discretize(host, name, inputs, outputs, source),
        "residue" => flatten_residue(host, inputs, outputs, source),
        "errorconst" => flatten_error_const(host, inputs, outputs, source),
        "mason" => flatten_mason(host, inputs, outputs, source),
        other => Err(parse_err(format!(
            "'{other}' is not a control-systems CALL intrinsic"
        ))),
    }
}

/// The control-systems arms of `EquationParser.autoSizeCallOutputs`: grow bare
/// output names into explicit slices sized from the inputs. The caller ignores
/// the error, exactly as the Java's `catch (ParseException ignored)` does — a
/// failure part way through leaves the earlier slots already sized.
pub fn auto_size<S: Shapes + ?Sized>(
    shapes: &S,
    name: &str,
    inputs: &[Expr],
    outputs: &mut [Expr],
) -> Result<()> {
    if !outputs.iter().any(|o| matches!(o, Expr::Var(_))) {
        return Ok(());
    }
    match name {
        "ss2ss" => {
            let n = in_mat_rows(shapes, inputs, 0)?;
            set_mat(outputs, 0, n, n);
            set_vec(outputs, 1, n);
            set_vec(outputs, 2, n);
        }
        "ss2tf" | "ss2tfij" => {
            let n = in_mat_rows(shapes, inputs, 0)?;
            set_vec(outputs, 0, n + 1);
            set_vec(outputs, 1, n + 1);
        }
        "tf2ss" => {
            let n = in_vec_len(shapes, inputs, 1)?.saturating_sub(1);
            set_mat(outputs, 0, n, n);
            set_vec(outputs, 1, n);
            set_vec(outputs, 2, n);
        }
        "zp2tf" => {
            let np = in_vec_len(shapes, inputs, 2)?;
            set_vec(outputs, 0, np + 1);
            set_vec(outputs, 1, np + 1);
        }
        "tf2zp" => {
            // pr/pi follow the denominator degree; zr/zi (the finite-zero
            // count) stay explicit.
            let np = in_vec_len(shapes, inputs, 1)?.saturating_sub(1);
            set_vec(outputs, 2, np);
            set_vec(outputs, 3, np);
        }
        "pole" => {
            let n = if inputs.len() == 1 {
                in_mat_rows(shapes, inputs, 0)?
            } else {
                in_vec_len(shapes, inputs, 1)?.saturating_sub(1)
            };
            set_vec(outputs, 0, n);
            set_vec(outputs, 1, n);
        }
        "zero" => {
            // Transfer-function form only: the finite-zero count follows the
            // numerator degree. State-space zero counts stay explicit.
            if inputs.len() == 2 {
                let nz = in_vec_len(shapes, inputs, 0)?.saturating_sub(1);
                set_vec(outputs, 0, nz);
                set_vec(outputs, 1, nz);
            }
        }
        "series" | "parallel" | "feedback" => {
            if inputs.len() >= 8 {
                let n = in_mat_rows(shapes, inputs, 0)? + in_mat_rows(shapes, inputs, 4)?;
                set_mat(outputs, 0, n, n);
                set_vec(outputs, 1, n);
                set_vec(outputs, 2, n);
            } else {
                let len = (in_vec_len(shapes, inputs, 0)? + in_vec_len(shapes, inputs, 2)?)
                    .saturating_sub(1);
                set_vec(outputs, 0, len);
                set_vec(outputs, 1, len);
            }
        }
        "bode" | "nyquist" | "nichols" => {
            let nf = in_vec_len(shapes, inputs, inputs.len().saturating_sub(1))?;
            set_vec(outputs, 0, nf);
            set_vec(outputs, 1, nf);
        }
        "lsim" => {
            let n = in_vec_len(shapes, inputs, inputs.len().saturating_sub(1))?;
            set_vec(outputs, 0, n);
        }
        "step" | "impulse" => {
            // (num, den) / (A, B, C, D) -> default grid; a trailing t matches t.
            let model_inputs = if inputs.len() >= 4 { 4 } else { 2 };
            let has_time = inputs.len() == model_inputs + 1;
            let n = if has_time {
                in_vec_len(shapes, inputs, inputs.len() - 1)?
            } else {
                DEFAULT_TIME_POINTS
            };
            set_vec(outputs, 0, n);
            if outputs.len() == 2 {
                set_vec(outputs, 1, n); // [y, t] captures the auto-generated grid
            }
        }
        "residue" => {
            let n = in_vec_len(shapes, inputs, 1)?.saturating_sub(1);
            set_vec(outputs, 0, n);
            set_vec(outputs, 1, n);
            set_vec(outputs, 2, n);
            set_vec(outputs, 3, n);
            if outputs.len() == 6 {
                set_vec(outputs, 4, n); // repeated-pole order vector
            }
        }
        "lqr" | "dlqr" | "place" | "acker" => {
            let n = in_mat_rows(shapes, inputs, 0)?;
            let m = in_mat_cols(shapes, inputs, 1)?;
            // A single-input system takes a 1×n gain; size it as a plain
            // n-vector K[1:n] — how SISO gains are written and used downstream
            // (`A - B*K`) — so bare/destructured outputs match. MIMO keeps the
            // m×n matrix.
            if m == 1 {
                set_vec(outputs, 0, n);
            } else {
                set_mat(outputs, 0, m, n);
            }
        }
        "dare" | "lyap" | "dlyap" | "gram" => {
            let n = in_mat_rows(shapes, inputs, 0)?;
            set_mat(outputs, 0, n, n);
        }
        "lqe" => {
            let n = in_mat_rows(shapes, inputs, 0)?;
            let p = in_mat_rows(shapes, inputs, 2)?; // C is p×n
            set_mat(outputs, 0, n, p);
        }
        "balreal" => {
            let n = in_mat_rows(shapes, inputs, 0)?;
            let m = in_mat_cols(shapes, inputs, 1)?; // B is n×m
            let p = in_mat_rows(shapes, inputs, 2)?; // C is p×n
            set_mat(outputs, 0, n, n);
            set_mat(outputs, 1, n, m);
            set_mat(outputs, 2, p, n);
        }
        "ctrb" => {
            let n = in_mat_rows(shapes, inputs, 0)?;
            let m = in_mat_cols(shapes, inputs, 1)?;
            set_mat(outputs, 0, n, n * m);
        }
        "obsv" => {
            let n = in_mat_rows(shapes, inputs, 0)?;
            let p = in_mat_rows(shapes, inputs, 1)?;
            set_mat(outputs, 0, n * p, n);
        }
        "pade" => {
            let m = in_scalar_int(shapes, inputs, 1)?.saturating_add(1);
            set_vec(outputs, 0, usize::try_from(m).unwrap_or(0));
            set_vec(outputs, 1, usize::try_from(m).unwrap_or(0));
        }
        "c2d" | "d2c" => {
            let len = in_vec_len(shapes, inputs, 1)?;
            set_vec(outputs, 0, len);
            set_vec(outputs, 1, len);
        }
        "rlocus" => {
            let order = in_vec_len(shapes, inputs, 1)?.saturating_sub(1);
            set_vec(outputs, 0, DEFAULT_RLOCUS_POINTS);
            set_mat(outputs, 1, DEFAULT_RLOCUS_POINTS, order);
            set_mat(outputs, 2, DEFAULT_RLOCUS_POINTS, order);
        }
        // `margin`, `routh`, `stepinfo`, `pidtune`, `rank`, `errorconst` and
        // `mason` have scalar or value-declared outputs: leave as written.
        _ => {}
    }
    Ok(())
}

/// The number of outputs a control-systems CALL produces, for
/// `EquationParser.padOmittedOutputs`. `None` when the count must be stated.
/// Port of the control arms of `EquationParser.expectedOutputCount`.
pub fn expected_output_count(name: &str, inputs: &[Expr]) -> Option<usize> {
    match name {
        "ss2tf" | "ss2tfij" | "zp2tf" | "c2d" | "d2c" | "pade" | "pole" | "zero" | "bode"
        | "nyquist" | "nichols" => Some(2),
        "tf2ss" | "margin" | "stepinfo" => Some(4),
        "tf2zp" => Some(5),
        "series" | "parallel" | "feedback" => Some(if inputs.len() >= 8 { 4 } else { 2 }),
        "ss2ss" => Some(3),
        "rlocus" | "errorconst" | "pidtune" | "balreal" => Some(3),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Shared argument readers (the Java's private helpers)
// ---------------------------------------------------------------------------

/// Port of `ControlSystemsFlattener.getMatrixData`: reads a 2-D slice as a
/// matrix, a 1-D slice as a column vector, a bare name as a 1×1 holding the
/// **unexpanded** variable, and a literal as a 1×1 named `scalar`.
fn matrix_data<S: Shapes + ?Sized>(shapes: &S, e: &Expr) -> Result<MatrixRef> {
    match e {
        Expr::ArrayAccess { indices, .. } if indices.len() == 2 => shapes.matrix_info(e),
        Expr::ArrayAccess { indices, .. } if indices.len() == 1 => {
            let v = shapes.vector_info(e)?;
            Ok(MatrixRef {
                name: v.name,
                rows: v.size,
                cols: 1,
                elements: v.elements.into_iter().map(|el| vec![el]).collect(),
            })
        }
        Expr::Var(name) => Ok(MatrixRef {
            name: name.clone(),
            rows: 1,
            cols: 1,
            elements: vec![vec![e.clone()]],
        }),
        Expr::Num { .. } => Ok(MatrixRef {
            name: "scalar".to_string(),
            rows: 1,
            cols: 1,
            elements: vec![vec![e.clone()]],
        }),
        other => Err(parse_err(format!(
            "Cannot parse {other:?} as a matrix, vector, or scalar."
        ))),
    }
}

/// Port of `getVectorElements`: a column of `expected` elements. The error text
/// names `ss2tf` for every caller — the Java hardcodes it.
fn vector_elements<S: Shapes + ?Sized>(shapes: &S, e: &Expr, expected: usize) -> Result<Vec<Expr>> {
    if let Expr::ArrayAccess { indices, .. } = e {
        if indices.len() == 2 {
            let m = shapes.matrix_info(e)?;
            if m.rows != expected || m.cols != 1 {
                return Err(parse_err(format!(
                    "ss2tf: B must be a vector of size {expected}x1 (got {}x{})",
                    m.rows, m.cols
                )));
            }
            return Ok((0..expected).map(|i| m.elements[i][0].clone()).collect());
        }
    }
    let v = shapes.vector_info(e)?;
    if v.size != expected {
        return Err(parse_err(format!(
            "ss2tf: B must be a vector of size {expected} (got size {})",
            v.size
        )));
    }
    Ok(v.elements)
}

/// Port of `getRowVectorElements`: a row of `expected` elements, same
/// hardcoded `ss2tf` wording.
fn row_vector_elements<S: Shapes + ?Sized>(
    shapes: &S,
    e: &Expr,
    expected: usize,
) -> Result<Vec<Expr>> {
    if let Expr::ArrayAccess { indices, .. } = e {
        if indices.len() == 2 {
            let m = shapes.matrix_info(e)?;
            if m.rows != 1 || m.cols != expected {
                return Err(parse_err(format!(
                    "ss2tf: C must be a row vector of size 1x{expected} (got {}x{})",
                    m.rows, m.cols
                )));
            }
            return Ok(m.elements[0].clone());
        }
    }
    let v = shapes.vector_info(e)?;
    if v.size != expected {
        return Err(parse_err(format!(
            "ss2tf: C must be a vector of size {expected} (got size {})",
            v.size
        )));
    }
    Ok(v.elements)
}

/// Port of `getScalarElement`: unwraps a 1×1 slice, a size-1 range, or an
/// ordinary scalar expression.
fn scalar_element<S: Shapes + ?Sized>(shapes: &S, e: &Expr) -> Result<Expr> {
    let Expr::ArrayAccess { indices, .. } = e else {
        return shapes.expand(e);
    };
    if indices.len() == 2 {
        let m = shapes.matrix_info(e)?;
        if m.rows != 1 || m.cols != 1 {
            return Err(parse_err(format!(
                "ss2tf: D must be a 1x1 matrix (got {}x{})",
                m.rows, m.cols
            )));
        }
        return Ok(m.elements[0][0].clone());
    }
    let index = shapes.expand(&indices[0])?;
    if matches!(index, Expr::Range { .. }) {
        let v = shapes.vector_info(e)?;
        if v.size != 1 {
            return Err(parse_err(format!(
                "ss2tf: D must be a size-1 vector (got size {})",
                v.size
            )));
        }
        return Ok(v.elements[0].clone());
    }
    shapes.expand(e)
}

/// The four `inMat*`/`inVec*`/`inScalar*` sizing probes `auto_size` shares with
/// `expand::Flattener`, restated here because they read through [`Shapes`].
fn in_mat_rows<S: Shapes + ?Sized>(shapes: &S, inputs: &[Expr], idx: usize) -> Result<usize> {
    let expr = input_at(inputs, idx)?;
    if let Expr::ArrayAccess { indices, .. } = expr {
        if indices.len() == 2 {
            return Ok(shapes.matrix_info(expr)?.rows);
        }
        if indices.len() == 1 {
            return Ok(shapes.vector_info(expr)?.size);
        }
    }
    Ok(1)
}

fn in_mat_cols<S: Shapes + ?Sized>(shapes: &S, inputs: &[Expr], idx: usize) -> Result<usize> {
    let expr = input_at(inputs, idx)?;
    if let Expr::ArrayAccess { indices, .. } = expr {
        if indices.len() == 2 {
            return Ok(shapes.matrix_info(expr)?.cols);
        }
        if indices.len() == 1 {
            return Ok(1);
        }
    }
    Ok(1)
}

fn in_vec_len<S: Shapes + ?Sized>(shapes: &S, inputs: &[Expr], idx: usize) -> Result<usize> {
    Ok(shapes.vector_info(input_at(inputs, idx)?)?.size)
}

fn in_scalar_int<S: Shapes + ?Sized>(shapes: &S, inputs: &[Expr], idx: usize) -> Result<i64> {
    shapes.const_index(input_at(inputs, idx)?)
}

fn input_at(inputs: &[Expr], idx: usize) -> Result<&Expr> {
    inputs
        .get(idx)
        .ok_or_else(|| parse_err(format!("CALL input {} is missing", idx + 1)))
}

fn output_at(outputs: &[Expr], idx: usize) -> Result<&Expr> {
    outputs
        .get(idx)
        .ok_or_else(|| parse_err(format!("CALL output {} is missing", idx + 1)))
}

/// `setVec`: a bare output name becomes `name[1:size]` (no-op on size 0).
fn set_vec(outputs: &mut [Expr], index: usize, size: usize) {
    if size == 0 {
        return;
    }
    if let Some(Expr::Var(name)) = outputs.get(index) {
        let name = name.clone();
        outputs[index] = Expr::ArrayAccess {
            name,
            indices: vec![range_one_to(size)],
        };
    }
}

/// `setMat`: a bare output name becomes `name[1:rows, 1:cols]`.
fn set_mat(outputs: &mut [Expr], index: usize, rows: usize, cols: usize) {
    if rows == 0 || cols == 0 {
        return;
    }
    if let Some(Expr::Var(name)) = outputs.get(index) {
        let name = name.clone();
        outputs[index] = Expr::ArrayAccess {
            name,
            indices: vec![range_one_to(rows), range_one_to(cols)],
        };
    }
}

fn range_one_to(n: usize) -> Expr {
    Expr::Range {
        start: Box::new(Expr::num(1.0)),
        end: Box::new(Expr::num(n as f64)),
    }
}

/// Registers the shape of an output written as an explicit slice. Port of the
/// `if (outputs.get(k) instanceof Expr.ArrayAccess aa) registerShape(...)`
/// guard the Java repeats at every vector-output site.
fn register_slice_shape<H: Host + ?Sized>(host: &mut H, out: &Expr, rows: usize, cols: usize) {
    if let Expr::ArrayAccess { name, .. } = out {
        let name = name.clone();
        host.register_shape(&name, rows, cols);
    }
}

/// Guards a write into a user-declared output matrix. The Java indexes
/// `outFlattened` blindly and throws `IndexOutOfBoundsException` when the
/// declared output is smaller than the shape the intrinsic produces; a wasm
/// engine reports it instead.
fn require_output_cells(op: &str, slots: usize, rows: usize, cols: usize) -> Result<()> {
    if slots < rows.saturating_mul(cols) {
        return Err(parse_err(format!(
            "{op}: output must have room for {rows}x{cols} = {} elements (got {slots})",
            rows.saturating_mul(cols)
        )));
    }
    Ok(())
}

/// Zero-pads a transfer-function numerator with leading zeros up to the
/// denominator length. Port of `ControlSystemsFlattener.padNumerator`: writing
/// `num=[1], den=[1,3,2]` instead of `num=[0,0,1]` is how control theory and
/// array languages spell a proper transfer function, and every downstream
/// synthetic assumes `num` carries `den.size` coefficients. A numerator
/// *longer* than the denominator is genuinely improper and is still refused.
fn pad_numerator(fname: &str, num: &VectorRef, den: &VectorRef) -> Result<Vec<Expr>> {
    if num.size > den.size {
        return Err(parse_err(format!(
            "{fname}: numerator is longer than the denominator (improper transfer function): \
             num has {} coefficients, den has {}",
            num.size, den.size
        )));
    }
    let mut padded = Vec::with_capacity(den.size);
    padded.extend((num.size..den.size).map(|_| Expr::num(0.0)));
    padded.extend(num.elements.iter().cloned());
    Ok(padded)
}

/// Serialises a SISO state-space model as `A` row-major, then `B`, then `C`,
/// then `D` — the layout `ControlSystemsEvaluator.ssArgsToNumDen` reconstructs.
/// Shared by `zero`, `bode`, `nyquist`, `nichols`, `margin`, `step`, `impulse`
/// and `lsim`, each of which names itself in the "A must be square" error.
fn ss_entries<S: Shapes + ?Sized>(shapes: &S, op: &str, inputs: &[Expr]) -> Result<Vec<Expr>> {
    let a = shapes.matrix_info(input_at(inputs, 0)?)?;
    let n = a.rows;
    if a.cols != n {
        return Err(parse_err(format!("{op}: A must be square")));
    }
    let b = vector_elements(shapes, input_at(inputs, 1)?, n)?;
    let c = row_vector_elements(shapes, input_at(inputs, 2)?, n)?;
    let d = scalar_element(shapes, input_at(inputs, 3)?)?;
    let mut entries = a.entries();
    entries.extend(b);
    entries.extend(c);
    entries.push(d);
    Ok(entries)
}

/// Serialises a transfer function as the zero-padded numerator followed by the
/// denominator.
fn tf_entries<S: Shapes + ?Sized>(shapes: &S, op: &str, inputs: &[Expr]) -> Result<Vec<Expr>> {
    let num = shapes.vector_info(input_at(inputs, 0)?)?;
    let den = shapes.vector_info(input_at(inputs, 1)?)?;
    let mut entries = pad_numerator(op, &num, &den)?;
    entries.extend(den.elements);
    Ok(entries)
}

// ---------------------------------------------------------------------------
// LTI conversions
// ---------------------------------------------------------------------------

/// `CALL rank(M : r)`. Port of `flattenRank`.
fn flatten_rank<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 1 || outputs.len() != 1 {
        return Err(parse_err(
            "rank expects 1 input (M) and 1 output (r), e.g. CALL rank(M[1:3,1:3] : r)",
        ));
    }
    let m = host.matrix_info(&inputs[0])?;
    let (rows, cols) = (m.rows, m.cols);
    let entries = m.entries();
    host.emit(Equation::new(
        outputs[0].clone(),
        Expr::Call {
            function: format!("rank${rows}${cols}"),
            args: entries,
        },
        source,
    ))
}

/// `CALL ss2ss(A, B, C, D, P : An, Bn, Cn, Dn)`. Port of `flattenSs2ss`.
fn flatten_ss2ss<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 5 || outputs.len() != 4 {
        return Err(parse_err(
            "ss2ss expects 5 inputs (A, B, C, D, P) and 4 outputs (An, Bn, Cn, Dn), \
             e.g. CALL ss2ss(A, B, C, D, P : An, Bn, Cn, Dn)",
        ));
    }
    let a = host.matrix_info(&inputs[0])?;
    let n = a.rows;
    if a.cols != n {
        return Err(parse_err("ss2ss: A must be square"));
    }
    let b = host.matrix_info(&inputs[1])?;
    let c = host.matrix_info(&inputs[2])?;
    let d = host.matrix_info(&inputs[3])?;
    let transform = host.matrix_info(&inputs[4])?;
    if transform.rows != n || transform.cols != n {
        return Err(parse_err(format!(
            "ss2ss: transform matrix P must be {n}x{n}"
        )));
    }

    let an = host.matrix_info(&outputs[0])?;
    if an.rows != n || an.cols != n {
        return Err(parse_err(format!("ss2ss: An must be {n}x{n}")));
    }
    let bn = host.matrix_info(&outputs[1])?;
    let cn = host.matrix_info(&outputs[2])?;
    let dn = host.matrix_info(&outputs[3])?;

    let m = d.cols;
    let p = d.rows;

    host.register_shape(&an.name, n, n);
    host.register_shape(&bn.name, n, m);
    host.register_shape(&cn.name, p, n);
    host.register_shape(&dn.name, p, m);

    require_output_cells("ss2ss", an.rows * an.cols, n, n)?;
    require_output_cells("ss2ss", bn.rows * bn.cols, n, m)?;
    require_output_cells("ss2ss", cn.rows * cn.cols, p, n)?;
    require_output_cells("ss2ss", dn.rows * dn.cols, p, m)?;
    host.reserve(n * n + n * m + p * n + p * m)?;

    let mut entries = a.entries();
    entries.extend(b.entries());
    entries.extend(c.entries());
    entries.extend(d.entries());
    entries.extend(transform.entries());

    let suffix = format!("${n}${m}${p}");
    for (tag, out, rows, cols) in [
        ("a", &an, n, n),
        ("b", &bn, n, m),
        ("c", &cn, p, n),
        ("d", &dn, p, m),
    ] {
        let slots = out.out_slots();
        let mut k = 0;
        for i in 0..rows {
            for j in 0..cols {
                host.emit(Equation::new(
                    slots[k].clone(),
                    Expr::Call {
                        function: format!("ss2ss${tag}${i}${j}{suffix}"),
                        args: entries.clone(),
                    },
                    source,
                ))?;
                k += 1;
            }
        }
    }
    Ok(())
}

/// `CALL ss2tf(A, B, C, D : num, den)`. Port of `flattenSs2tf`.
/// Ceiling on the state count reaching the `ss2tf$` kernel (Wave C4, closing
/// Phase 9's gap 5). The Leverrier recursion behind the kernel is measured at
/// **5.0 s for 201 states** — the slowest single operation Phase 9 found —
/// and nothing upstream bounded `n`, because the per-cell synthetic re-runs
/// the whole recursion for every coefficient of every Newton residual sweep.
/// The corpus's real usage tops out below 10 states; 64 leaves an order of
/// magnitude of margin while keeping the worst admissible case ~100× cheaper
/// than the measured cliff. The Java has no ceiling here — a wasm-native
/// guard in the `measurement.rs` MAX_INPUTS tradition, refusing loudly at
/// parse time rather than spinning a worker.
const MAX_SS2TF_STATES: usize = 64;

fn check_ss2tf_states(n: usize) -> Result<()> {
    if n > MAX_SS2TF_STATES {
        return Err(parse_err(format!(
            "ss2tf: the state matrix has too many states ({n}; limit \
             {MAX_SS2TF_STATES}). Reduce the model order — the per-coefficient \
             expansion re-runs a full Leverrier recursion for every equation \
             at this size."
        )));
    }
    Ok(())
}

fn flatten_ss2tf<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 4 || outputs.len() != 2 {
        return Err(parse_err(
            "ss2tf expects 4 inputs (A, B, C, D) and 2 outputs (num, den), \
             e.g. CALL ss2tf(A[1:2,1:2], B[1:2], C[1:2], D : num[1:3], den[1:3])",
        ));
    }
    let a = host.matrix_info(&inputs[0])?;
    let n = a.rows;
    if a.cols != n {
        return Err(parse_err(format!(
            "ss2tf: A must be square (got {}x{})",
            a.rows, a.cols
        )));
    }
    check_ss2tf_states(n)?;
    let b = vector_elements(host, &inputs[1], n)?;
    let c = row_vector_elements(host, &inputs[2], n)?;
    let d = scalar_element(host, &inputs[3])?;

    // The order `eval::ss2tf` reconstructs: A row-major, then B, then C, then D.
    let mut entries = a.entries();
    entries.extend(b);
    entries.extend(c);
    entries.push(d);

    emit_ss2tf_outputs(host, n, outputs, &entries, source)
}

/// `CALL ss2tfij(A, B, C, D, i, j : num, den)`.
///
/// The channel from input `j` to output `i` of a multivariable state space is
/// `C_i·(sI−A)⁻¹·B_j + D_ij` — the SISO formula on row `i` of C and column `j`
/// of B — so this selects that row/column and reuses the SISO `ss2tf$`
/// evaluator. Port of `flattenSs2tfMimo`.
fn flatten_ss2tf_mimo<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 6 || outputs.len() != 2 {
        return Err(parse_err(
            "ss2tfij expects 6 inputs (A, B, C, D, i, j) and 2 outputs (num, den), \
             e.g. CALL ss2tfij(A[1:2,1:2], B[1:2,1:2], C[1:2,1:2], D[1:2,1:2], 1, 1 : \
             num[1:3], den[1:3])",
        ));
    }
    let a = host.matrix_info(&inputs[0])?;
    let n = a.rows;
    if a.cols != n {
        return Err(parse_err(format!(
            "ss2tfij: A must be square (got {}x{})",
            a.rows, a.cols
        )));
    }
    check_ss2tf_states(n)?;
    let b = host.matrix_info(&inputs[1])?;
    let cm = host.matrix_info(&inputs[2])?;
    let dm = host.matrix_info(&inputs[3])?;
    let iout = host.const_index(&inputs[4])?; // 1-based output index
    let jin = host.const_index(&inputs[5])?; // 1-based input index
    if b.rows != n {
        return Err(parse_err(format!(
            "ss2tfij: B must have n={n} rows (got {}).",
            b.rows
        )));
    }
    if cm.cols != n {
        return Err(parse_err(format!(
            "ss2tfij: C must have n={n} columns (got {}).",
            cm.cols
        )));
    }
    if iout < 1 || iout > cm.rows as i64 {
        return Err(parse_err(format!(
            "ss2tfij: output index i={iout} out of range 1..{}.",
            cm.rows
        )));
    }
    if jin < 1 || jin > b.cols as i64 {
        return Err(parse_err(format!(
            "ss2tfij: input index j={jin} out of range 1..{}.",
            b.cols
        )));
    }
    if dm.rows != cm.rows || dm.cols != b.cols {
        return Err(parse_err(format!(
            "ss2tfij: D must be q×p ({}x{}).",
            cm.rows, b.cols
        )));
    }
    let iout = (iout - 1) as usize;
    let jin = (jin - 1) as usize;

    // Entries in the order `eval::ss2tf` reconstructs: A row-major, B column j,
    // C row i, D_ij.
    let mut entries = a.entries();
    entries.extend((0..n).map(|r| b.elements[r][jin].clone()));
    entries.extend((0..n).map(|col| cm.elements[iout][col].clone()));
    entries.push(dm.elements[iout][jin].clone());

    let num = host.vector_info(&outputs[0])?;
    let den = host.vector_info(&outputs[1])?;
    if num.size != n + 1 || den.size != n + 1 {
        return Err(parse_err(format!(
            "ss2tfij: num and den outputs must each have length n+1 = {}",
            n + 1
        )));
    }
    register_slice_shape(host, &outputs[0], n + 1, 1);
    register_slice_shape(host, &outputs[1], n + 1, 1);
    host.reserve(2 * (n + 1))?;
    for k in 0..=n {
        host.emit(Equation::new(
            num.elements[k].clone(),
            Expr::Call {
                function: format!("ss2tf$num${k}${n}"),
                args: entries.clone(),
            },
            source,
        ))?;
        host.emit(Equation::new(
            den.elements[k].clone(),
            Expr::Call {
                function: format!("ss2tf$den${k}${n}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    Ok(())
}

/// The shared `num[k] = ss2tf$num$k$n(...)` / `den[k] = …` tail of both ss2tf
/// forms.
fn emit_ss2tf_outputs<H: Host + ?Sized>(
    host: &mut H,
    n: usize,
    outputs: &[Expr],
    entries: &[Expr],
    source: &str,
) -> Result<()> {
    let num = host.vector_info(&outputs[0])?;
    let den = host.vector_info(&outputs[1])?;
    if num.size != n + 1 || den.size != n + 1 {
        return Err(parse_err(format!(
            "ss2tf: num and den outputs must each have length n+1 = {} \
             (e.g. num[1:{}], den[1:{}])",
            n + 1,
            n + 1,
            n + 1
        )));
    }
    register_slice_shape(host, &outputs[0], n + 1, 1);
    register_slice_shape(host, &outputs[1], n + 1, 1);
    host.reserve(2 * (n + 1))?;
    for k in 0..=n {
        host.emit(Equation::new(
            num.elements[k].clone(),
            Expr::Call {
                function: format!("ss2tf$num${k}${n}"),
                args: entries.to_vec(),
            },
            source,
        ))?;
        host.emit(Equation::new(
            den.elements[k].clone(),
            Expr::Call {
                function: format!("ss2tf$den${k}${n}"),
                args: entries.to_vec(),
            },
            source,
        ))?;
    }
    Ok(())
}

/// `CALL tf2ss(num, den : A, B, C, D)`. Port of `flattenTf2ss`.
fn flatten_tf2ss<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 2 || outputs.len() != 4 {
        return Err(parse_err(
            "tf2ss expects 2 inputs (num, den) and 4 outputs (A, B, C, D), \
             e.g. CALL tf2ss(num[1:3], den[1:3] : A[1:2,1:2], B[1:2], C[1:2], D)",
        ));
    }
    let num = host.vector_info(&inputs[0])?;
    let den = host.vector_info(&inputs[1])?;
    let num_padded = pad_numerator("tf2ss", &num, &den)?;
    let n = den.size.saturating_sub(1);

    let a = host.matrix_info(&outputs[0])?;
    if a.rows != n || a.cols != n {
        return Err(parse_err(format!("tf2ss: A must be n x n = {n}x{n}")));
    }
    let b = vector_elements(host, &outputs[1], n)?;
    let c = row_vector_elements(host, &outputs[2], n)?;
    let d = scalar_element(host, &outputs[3])?;

    host.register_shape(&a.name, a.rows, a.cols);
    register_slice_shape(host, &outputs[1], n, 1);
    register_slice_shape(host, &outputs[2], 1, n);

    let mut entries = num_padded;
    entries.extend(den.elements);

    host.reserve(n * n + 2 * n + 1)?;
    for i in 0..n {
        for j in 0..n {
            host.emit(Equation::new(
                a.elements[i][j].clone(),
                Expr::Call {
                    function: format!("tf2ss$a${i}${j}${n}"),
                    args: entries.clone(),
                },
                source,
            ))?;
        }
        host.emit(Equation::new(
            b[i].clone(),
            Expr::Call {
                function: format!("tf2ss$b${i}${n}"),
                args: entries.clone(),
            },
            source,
        ))?;
        host.emit(Equation::new(
            c[i].clone(),
            Expr::Call {
                function: format!("tf2ss$c${i}${n}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    host.emit(Equation::new(
        d,
        Expr::Call {
            function: format!("tf2ss$d${n}"),
            args: entries,
        },
        source,
    ))
}

/// `CALL zp2tf(z_r, z_i, p_r, p_i, k : num, den)`. Port of `flattenZp2tf`.
fn flatten_zp2tf<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 5 || outputs.len() != 2 {
        return Err(parse_err(
            "zp2tf expects 5 inputs (z_r, z_i, p_r, p_i, k) and 2 outputs (num, den), \
             e.g. CALL zp2tf(z_r[1:2], z_i[1:2], p_r[1:2], p_i[1:2], k : num[1:3], den[1:3])",
        ));
    }
    let zr = host.vector_info(&inputs[0])?;
    let zi = host.vector_info(&inputs[1])?;
    let pr = host.vector_info(&inputs[2])?;
    let pi = host.vector_info(&inputs[3])?;
    let k_expr = scalar_element(host, &inputs[4])?;

    if zr.size != zi.size {
        return Err(parse_err("zp2tf: z_r and z_i must have the same length"));
    }
    if pr.size != pi.size {
        return Err(parse_err("zp2tf: p_r and p_i must have the same length"));
    }
    let nz = zr.size;
    let np = pr.size;

    let num = host.vector_info(&outputs[0])?;
    let den = host.vector_info(&outputs[1])?;
    if num.size != np + 1 || den.size != np + 1 {
        return Err(parse_err(format!(
            "zp2tf: num and den must have length np + 1 = {}",
            np + 1
        )));
    }
    register_slice_shape(host, &outputs[0], np + 1, 1);
    register_slice_shape(host, &outputs[1], np + 1, 1);

    let mut entries = zr.elements;
    entries.extend(zi.elements);
    entries.extend(pr.elements);
    entries.extend(pi.elements);
    entries.push(k_expr);

    host.reserve(2 * (np + 1))?;
    for i in 0..=np {
        host.emit(Equation::new(
            num.elements[i].clone(),
            Expr::Call {
                function: format!("zp2tf$num${i}${nz}${np}"),
                args: entries.clone(),
            },
            source,
        ))?;
        host.emit(Equation::new(
            den.elements[i].clone(),
            Expr::Call {
                function: format!("zp2tf$den${i}${nz}${np}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    Ok(())
}

/// `CALL tf2zp(num, den : z_r, z_i, p_r, p_i, k)`. Port of `flattenTf2zp`.
fn flatten_tf2zp<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 2 || outputs.len() != 5 {
        return Err(parse_err(
            "tf2zp expects 2 inputs (num, den) and 5 outputs (z_r, z_i, p_r, p_i, k), \
             e.g. CALL tf2zp(num[1:3], den[1:3] : z_r[1:2], z_i[1:2], p_r[1:2], p_i[1:2], k)",
        ));
    }
    let num = host.vector_info(&inputs[0])?;
    let den = host.vector_info(&inputs[1])?;
    let np = den.size.saturating_sub(1); // denominator degree

    let zr = host.vector_info(&outputs[0])?;
    let zi = host.vector_info(&outputs[1])?;
    let pr = host.vector_info(&outputs[2])?;
    let pi = host.vector_info(&outputs[3])?;
    let k_expr = scalar_element(host, &outputs[4])?;

    if zr.size != zi.size {
        return Err(parse_err(
            "tf2zp: z_r and z_i outputs must have the same length",
        ));
    }
    if pr.size != pi.size {
        return Err(parse_err(
            "tf2zp: p_r and p_i outputs must have the same length",
        ));
    }
    let nz = zr.size;
    if pr.size != np {
        return Err(parse_err(format!(
            "tf2zp: p_r/p_i length must match denominator degree np = {np}"
        )));
    }

    register_slice_shape(host, &outputs[0], nz, 1);
    register_slice_shape(host, &outputs[1], nz, 1);
    register_slice_shape(host, &outputs[2], np, 1);
    register_slice_shape(host, &outputs[3], np, 1);

    let mut entries = num.elements;
    entries.extend(den.elements);

    host.reserve(2 * nz + 2 * np + 1)?;
    for i in 0..nz {
        host.emit(Equation::new(
            zr.elements[i].clone(),
            Expr::Call {
                function: format!("tf2zp$zr${i}${nz}${np}"),
                args: entries.clone(),
            },
            source,
        ))?;
        host.emit(Equation::new(
            zi.elements[i].clone(),
            Expr::Call {
                function: format!("tf2zp$zi${i}${nz}${np}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    for i in 0..np {
        host.emit(Equation::new(
            pr.elements[i].clone(),
            Expr::Call {
                function: format!("tf2zp$pr${i}${nz}${np}"),
                args: entries.clone(),
            },
            source,
        ))?;
        host.emit(Equation::new(
            pi.elements[i].clone(),
            Expr::Call {
                function: format!("tf2zp$pi${i}${nz}${np}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    host.emit(Equation::new(
        k_expr,
        Expr::Call {
            function: format!("tf2zp$k${nz}${np}"),
            args: entries,
        },
        source,
    ))
}

// ---------------------------------------------------------------------------
// Interconnection
// ---------------------------------------------------------------------------

/// Shared expansion for the two-system transfer-function interconnections
/// (`series`, `parallel`): both consume `(num1, den1, num2, den2)`, produce a
/// length `L1+L2-1` `(num, den)`, and differ only in the per-coefficient
/// backing function the solver evaluates. Port of `flattenTfCombine`.
fn flatten_tf_combine<H: Host + ?Sized>(
    host: &mut H,
    op: &str,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 4 || outputs.len() != 2 {
        return Err(parse_err(format!(
            "{op} expects 4 inputs (num1, den1, num2, den2) and 2 outputs (num, den), \
             e.g. CALL {op}(num1[1:2], den1[1:2], num2[1:2], den2[1:2] : num[1:3], den[1:3])"
        )));
    }
    let (entries, expected_len, l1, l2) = tf_combine_entries(host, op, inputs, None)?;
    emit_tf_combine(host, op, outputs, &entries, expected_len, l1, l2, source)
}

/// `CALL feedback(num1, den1, num2, den2 [, sign] : num, den)`. Port of
/// `flattenFeedback` — the only difference from [`flatten_tf_combine`] is the
/// optional trailing sign, appended to the serialised entries.
fn flatten_feedback<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if (inputs.len() != 4 && inputs.len() != 5) || outputs.len() != 2 {
        return Err(parse_err(
            "feedback expects 4 or 5 inputs (num1, den1, num2, den2, [sign]) and 2 outputs \
             (num, den), e.g. CALL feedback(num1[1:2], den1[1:2], num2[1:2], den2[1:2] : \
             num[1:3], den[1:3])",
        ));
    }
    let sign = if inputs.len() == 5 {
        Some(scalar_element(host, &inputs[4])?)
    } else {
        Some(Expr::num(1.0))
    };
    let (entries, expected_len, l1, l2) = tf_combine_entries(host, "feedback", inputs, sign)?;
    emit_tf_combine(
        host,
        "feedback",
        outputs,
        &entries,
        expected_len,
        l1,
        l2,
        source,
    )
}

/// Reads and validates `(num1, den1, num2, den2)`, returning the serialised
/// entries (with `sign` appended when present), the output length and the two
/// input lengths.
fn tf_combine_entries<H: Host + ?Sized>(
    host: &H,
    op: &str,
    inputs: &[Expr],
    sign: Option<Expr>,
) -> Result<(Vec<Expr>, usize, usize, usize)> {
    let num1 = host.vector_info(&inputs[0])?;
    let den1 = host.vector_info(&inputs[1])?;
    let num2 = host.vector_info(&inputs[2])?;
    let den2 = host.vector_info(&inputs[3])?;
    if num1.size != den1.size {
        return Err(parse_err(format!(
            "{op}: num1 and den1 must have the same length"
        )));
    }
    if num2.size != den2.size {
        return Err(parse_err(format!(
            "{op}: num2 and den2 must have the same length"
        )));
    }
    let l1 = num1.size;
    let l2 = num2.size;
    let expected_len = l1 + l2 - 1;
    let mut entries = num1.elements;
    entries.extend(den1.elements);
    entries.extend(num2.elements);
    entries.extend(den2.elements);
    if let Some(sign) = sign {
        entries.push(sign);
    }
    Ok((entries, expected_len, l1, l2))
}

#[allow(clippy::too_many_arguments)] // one parameter per Java local; splitting hides the layout
fn emit_tf_combine<H: Host + ?Sized>(
    host: &mut H,
    op: &str,
    outputs: &[Expr],
    entries: &[Expr],
    expected_len: usize,
    l1: usize,
    l2: usize,
    source: &str,
) -> Result<()> {
    let num = host.vector_info(&outputs[0])?;
    let den = host.vector_info(&outputs[1])?;
    if num.size != expected_len || den.size != expected_len {
        return Err(parse_err(format!(
            "{op}: outputs num and den must have length L1 + L2 - 1 = {expected_len}"
        )));
    }
    register_slice_shape(host, &outputs[0], expected_len, 1);
    register_slice_shape(host, &outputs[1], expected_len, 1);
    host.reserve(2 * expected_len)?;
    for i in 0..expected_len {
        host.emit(Equation::new(
            num.elements[i].clone(),
            Expr::Call {
                function: format!("{op}$num${i}${l1}${l2}"),
                args: entries.to_vec(),
            },
            source,
        ))?;
        host.emit(Equation::new(
            den.elements[i].clone(),
            Expr::Call {
                function: format!("{op}$den${i}${l1}${l2}"),
                args: entries.to_vec(),
            },
            source,
        ))?;
    }
    Ok(())
}

/// Two state-space realizations, read in the `A1 B1 C1 D1 A2 B2 C2 D2` order
/// `eval::ss_combine` reconstructs.
struct SsPair {
    n1: usize,
    p1: usize,
    q1: usize,
    n2: usize,
    p2: usize,
    q2: usize,
    entries: Vec<Expr>,
}

fn read_ss_pair<H: Host + ?Sized>(host: &H, op: &str, inputs: &[Expr]) -> Result<SsPair> {
    let a1 = host.matrix_info(&inputs[0])?;
    let n1 = a1.rows;
    if a1.cols != n1 {
        return Err(parse_err(format!("{op}: A1 must be square")));
    }
    let b1 = host.matrix_info(&inputs[1])?;
    let c1 = host.matrix_info(&inputs[2])?;
    let d1 = host.matrix_info(&inputs[3])?;
    let p1 = b1.cols;
    let q1 = c1.rows;

    let a2 = host.matrix_info(&inputs[4])?;
    let n2 = a2.rows;
    if a2.cols != n2 {
        return Err(parse_err(format!("{op}: A2 must be square")));
    }
    let b2 = host.matrix_info(&inputs[5])?;
    let c2 = host.matrix_info(&inputs[6])?;
    let d2 = host.matrix_info(&inputs[7])?;
    let p2 = b2.cols;
    let q2 = c2.rows;

    let mut entries = a1.entries();
    entries.extend(b1.entries());
    entries.extend(c1.entries());
    entries.extend(d1.entries());
    entries.extend(a2.entries());
    entries.extend(b2.entries());
    entries.extend(c2.entries());
    entries.extend(d2.entries());

    Ok(SsPair {
        n1,
        p1,
        q1,
        n2,
        p2,
        q2,
        entries,
    })
}

/// Shared expansion for the two-system state-space interconnections: both stack
/// the realizations identically and differ only in the per-element backing
/// function (`ss_<op>$…`). Port of `flattenSsCombine`.
///
/// The Java computes `p_out`/`q_out` with a `Math.max` arm and a `feedback`
/// arm, then unconditionally overwrites both for `parallel`/`feedback`; only
/// the surviving values are transcribed here — series takes `(p1, q2)`,
/// parallel takes `(p1, q1)`.
fn flatten_ss_combine<H: Host + ?Sized>(
    host: &mut H,
    op: &str,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 8 || outputs.len() != 4 {
        return Err(parse_err(format!(
            "{op} for state-space expects 8 inputs (A1, B1, C1, D1, A2, B2, C2, D2) and \
             4 outputs (A, B, C, D)"
        )));
    }
    let ss = read_ss_pair(host, op, inputs)?;
    let q_out = if op == "series" { ss.q2 } else { ss.q1 };
    emit_ss_combine(
        host,
        &format!("ss_{op}"),
        op,
        &ss,
        ss.p1,
        q_out,
        outputs,
        &ss.entries,
        source,
    )
}

/// `CALL feedback(A1, …, D2 [, sign] : A, B, C, D)`. Port of
/// `flattenSsFeedback` — the sign is appended to the entries **unexpanded**,
/// as the Java does (`inputs.get(8)` verbatim).
fn flatten_ss_feedback<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if (inputs.len() != 8 && inputs.len() != 9) || outputs.len() != 4 {
        return Err(parse_err(
            "feedback for state-space expects 8 or 9 inputs (A1, B1, C1, D1, A2, B2, C2, D2 \
             [, sign]) and 4 outputs (A, B, C, D)",
        ));
    }
    let ss = read_ss_pair(host, "feedback", inputs)?;
    let sign = if inputs.len() == 9 {
        inputs[8].clone()
    } else {
        Expr::num(1.0)
    };
    let mut entries = ss.entries.clone();
    entries.push(sign);
    emit_ss_combine(
        host,
        "ss_feedback",
        "feedback",
        &ss,
        ss.p1,
        ss.q1,
        outputs,
        &entries,
        source,
    )
}

#[allow(clippy::too_many_arguments)] // mirrors the Java local set; grouping would hide the layout
fn emit_ss_combine<H: Host + ?Sized>(
    host: &mut H,
    fname: &str,
    op: &str,
    ss: &SsPair,
    p_out: usize,
    q_out: usize,
    outputs: &[Expr],
    entries: &[Expr],
    source: &str,
) -> Result<()> {
    let n = ss.n1 + ss.n2;
    let an = host.matrix_info(&outputs[0])?;
    if an.rows != n || an.cols != n {
        return Err(parse_err(format!("{op}: Output A must be {n}x{n}")));
    }
    let bn = host.matrix_info(&outputs[1])?;
    let cn = host.matrix_info(&outputs[2])?;
    let dn = host.matrix_info(&outputs[3])?;

    host.register_shape(&an.name, n, n);
    host.register_shape(&bn.name, n, p_out);
    host.register_shape(&cn.name, q_out, n);
    host.register_shape(&dn.name, q_out, p_out);

    // The Java loops over the *declared* output extents for B, C and D (only A
    // is checked against n×n), so a mis-sized declaration simply writes fewer
    // or more equations. Transcribed as-is.
    host.reserve(n * n + bn.rows * bn.cols + cn.rows * cn.cols + dn.rows * dn.cols)?;
    let suffix = format!(
        "${}${}${}${}${}${}",
        ss.n1, ss.p1, ss.q1, ss.n2, ss.p2, ss.q2
    );
    for (tag, out, rows, cols) in [
        ("a", &an, n, n),
        ("b", &bn, bn.rows, bn.cols),
        ("c", &cn, cn.rows, cn.cols),
        ("d", &dn, dn.rows, dn.cols),
    ] {
        let slots = out.out_slots();
        require_output_cells(op, slots.len(), rows, cols)?;
        let mut k = 0;
        for i in 0..rows {
            for j in 0..cols {
                host.emit(Equation::new(
                    slots[k].clone(),
                    Expr::Call {
                        function: format!("{fname}${tag}${i}${j}{suffix}"),
                        args: entries.to_vec(),
                    },
                    source,
                ))?;
                k += 1;
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Poles, zeros and frequency response
// ---------------------------------------------------------------------------

/// `CALL pole(A : pr, pi)` or `CALL pole(num, den : pr, pi)`. Port of
/// `flattenPole`.
fn flatten_pole<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if (inputs.len() != 1 && inputs.len() != 2) || outputs.len() != 2 {
        return Err(parse_err(
            "pole expects 1 input (A) or 2 inputs (num, den) and 2 outputs (pr, pi), \
             e.g. CALL pole(num, den : pr[1:3], pi[1:3])",
        ));
    }
    let num_inputs = inputs.len();
    let (n, entries) = if num_inputs == 1 {
        let a = host.matrix_info(&inputs[0])?;
        if a.rows != a.cols {
            return Err(parse_err("pole: A must be square"));
        }
        (a.rows, a.entries())
    } else {
        // Java order: num, then den, then the degree, then the padded entries.
        let num = host.vector_info(&inputs[0])?;
        let den = host.vector_info(&inputs[1])?;
        let n = den.size.saturating_sub(1); // degree
        let mut entries = pad_numerator("pole", &num, &den)?;
        entries.extend(den.elements);
        (n, entries)
    };

    let pr = host.vector_info(&outputs[0])?;
    let pi = host.vector_info(&outputs[1])?;
    if pr.size != n || pi.size != n {
        return Err(parse_err(format!(
            "pole: output vectors pr and pi must have length n = {n}"
        )));
    }
    register_slice_shape(host, &outputs[0], n, 1);
    register_slice_shape(host, &outputs[1], n, 1);

    host.reserve(2 * n)?;
    for i in 0..n {
        host.emit(Equation::new(
            pr.elements[i].clone(),
            Expr::Call {
                function: format!("pole$pr${i}${num_inputs}${n}"),
                args: entries.clone(),
            },
            source,
        ))?;
        host.emit(Equation::new(
            pi.elements[i].clone(),
            Expr::Call {
                function: format!("pole$pi${i}${num_inputs}${n}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    Ok(())
}

/// `CALL zero(num, den : zr, zi)` or `CALL zero(A, B, C, D : zr, zi)`. Port of
/// `flattenZero`.
fn flatten_zero<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if (inputs.len() != 2 && inputs.len() != 4) || outputs.len() != 2 {
        return Err(parse_err(
            "zero expects 2 inputs (num, den) or 4 inputs (A, B, C, D) and 2 outputs (zr, zi), \
             e.g. CALL zero(num, den : zr[1:2], zi[1:2])",
        ));
    }
    let zr = host.vector_info(&outputs[0])?;
    let zi = host.vector_info(&outputs[1])?;
    if zr.size != zi.size {
        return Err(parse_err(
            "zero: zr and zi outputs must have the same length",
        ));
    }
    let nz = zr.size;
    let num_inputs = inputs.len();
    let entries = if num_inputs == 2 {
        tf_entries(host, "zero", inputs)?
    } else {
        ss_entries(host, "zero", inputs)?
    };

    register_slice_shape(host, &outputs[0], nz, 1);
    register_slice_shape(host, &outputs[1], nz, 1);

    host.reserve(2 * nz)?;
    for i in 0..nz {
        host.emit(Equation::new(
            zr.elements[i].clone(),
            Expr::Call {
                function: format!("zero$zr${i}${num_inputs}${nz}"),
                args: entries.clone(),
            },
            source,
        ))?;
        host.emit(Equation::new(
            zi.elements[i].clone(),
            Expr::Call {
                function: format!("zero$zi${i}${num_inputs}${nz}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    Ok(())
}

/// `bode`, `nyquist` and `nichols` share one shape: 3 inputs
/// `(num, den, omega)` or 5 inputs `(A, B, C, D, omega)`, two N-length
/// outputs, and per-sample synthetics `<op>$<partA|partB>$<i>$<numInputs>$<N>`.
/// Port of `flattenBode` / `flattenNyquist` / `flattenNichols`, which are
/// character-for-character identical apart from those two output tags.
fn flatten_freq_response<H: Host + ?Sized>(
    host: &mut H,
    op: &str,
    tag0: &str,
    tag1: &str,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if (inputs.len() != 3 && inputs.len() != 5) || outputs.len() != 2 {
        return Err(parse_err(format!(
            "{op} expects 3 inputs (num, den, omega) or 5 inputs (A, B, C, D, omega) and \
             2 outputs ({tag0}, {tag1}), e.g. CALL {op}(num, den, omega : {tag0}[1:50], \
             {tag1}[1:50])"
        )));
    }
    let num_inputs = inputs.len();
    let omega = host.vector_info(&inputs[num_inputs - 1])?;
    let n_pts = omega.size;

    let out0 = host.vector_info(&outputs[0])?;
    let out1 = host.vector_info(&outputs[1])?;
    if out0.size != n_pts || out1.size != n_pts {
        return Err(parse_err(format!(
            "{op}: outputs {tag0} and {tag1} must have the same size N as omega = {n_pts}"
        )));
    }

    let mut entries = if num_inputs == 3 {
        tf_entries(host, op, inputs)?
    } else {
        ss_entries(host, op, inputs)?
    };
    entries.extend(omega.elements);

    register_slice_shape(host, &outputs[0], n_pts, 1);
    register_slice_shape(host, &outputs[1], n_pts, 1);

    host.reserve(2 * n_pts)?;
    for i in 0..n_pts {
        host.emit(Equation::new(
            out0.elements[i].clone(),
            Expr::Call {
                function: format!("{op}${tag0}${i}${num_inputs}${n_pts}"),
                args: entries.clone(),
            },
            source,
        ))?;
        host.emit(Equation::new(
            out1.elements[i].clone(),
            Expr::Call {
                function: format!("{op}${tag1}${i}${num_inputs}${n_pts}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    Ok(())
}

/// `CALL margin(num, den : gm, pm, w_cg, w_cp)` — or the 4-input state-space
/// form. Port of `flattenMargin`.
fn flatten_margin<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if (inputs.len() != 2 && inputs.len() != 4) || outputs.len() != 4 {
        return Err(parse_err(
            "margin expects 2 inputs (num, den) or 4 inputs (A, B, C, D) and 4 scalar outputs \
             (gm, pm, w_cg, w_cp), e.g. CALL margin(num, den : gm, pm, w_cg, w_cp)",
        ));
    }
    let num_inputs = inputs.len();
    let entries = if num_inputs == 2 {
        tf_entries(host, "margin", inputs)?
    } else {
        ss_entries(host, "margin", inputs)?
    };
    for (k, part) in ["gm", "pm", "wcg", "wcp"].iter().enumerate() {
        host.emit(Equation::new(
            outputs[k].clone(),
            Expr::Call {
                function: format!("margin${part}${num_inputs}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    Ok(())
}

/// `CALL routh(den : nRHP, stable)`. Port of `flattenRouth` — the
/// characteristic-polynomial coefficients (descending powers) become the
/// synthetic's arguments.
fn flatten_routh<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 1 || outputs.len() != 2 {
        return Err(parse_err(
            "routh expects 1 input (den) and 2 scalar outputs (nRHP, stable), \
             e.g. CALL routh(den[1:4] : nRHP, stable)",
        ));
    }
    let den = host.vector_info(&inputs[0])?;
    let len = den.size;
    let entries = den.elements;
    for (k, part) in ["nrhp", "stable"].iter().enumerate() {
        host.emit(Equation::new(
            outputs[k].clone(),
            Expr::Call {
                function: format!("routh${part}${len}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    Ok(())
}

/// `CALL residue(num, den : r_r, r_i, p_r, p_i [, ord], k)`. Port of
/// `flattenResidue`: the 6-output form carries the per-term power `k` of each
/// `A/(s-p)^k`, which repeated poles require.
fn flatten_residue<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    let with_order = outputs.len() == 6;
    if inputs.len() != 2 || (outputs.len() != 5 && !with_order) {
        return Err(parse_err(
            "residue expects 2 inputs (num, den) and 5 outputs (r_r, r_i, p_r, p_i, k) \
             or 6 outputs (r_r, r_i, p_r, p_i, ord, k) for repeated poles, \
             e.g. CALL residue(num[1:1], den[1:3] : r_r[1:2], r_i[1:2], p_r[1:2], p_i[1:2], k)",
        ));
    }
    let num = host.vector_info(&inputs[0])?;
    let den = host.vector_info(&inputs[1])?;
    let n = den.size.saturating_sub(1); // residue terms = denominator degree

    let rr = host.vector_info(&outputs[0])?;
    let ri = host.vector_info(&outputs[1])?;
    let pr = host.vector_info(&outputs[2])?;
    let pi = host.vector_info(&outputs[3])?;
    if rr.size != n || ri.size != n || pr.size != n || pi.size != n {
        return Err(parse_err(format!(
            "residue: output vectors r_r, r_i, p_r, p_i must have length n = {n}"
        )));
    }
    let mut array_outputs = 4;
    let ord = if with_order {
        let ord = host.vector_info(&outputs[4])?;
        if ord.size != n {
            return Err(parse_err(format!(
                "residue: output vector ord must have length n = {n}"
            )));
        }
        array_outputs = 5;
        Some(ord)
    } else {
        None
    };

    let num_len = num.size;
    let mut entries = num.elements;
    entries.extend(den.elements);

    for out in outputs.iter().take(array_outputs) {
        register_slice_shape(host, out, n, 1);
    }

    let form = if with_order { "o" } else { "s" };
    host.reserve(array_outputs * n + 1)?;
    for i in 0..n {
        for (which, target) in [
            ("rr", &rr.elements[i]),
            ("ri", &ri.elements[i]),
            ("pr", &pr.elements[i]),
            ("pi", &pi.elements[i]),
        ] {
            host.emit(Equation::new(
                target.clone(),
                Expr::Call {
                    function: format!("residue${which}${form}${i}${num_len}${n}"),
                    args: entries.clone(),
                },
                source,
            ))?;
        }
        if let Some(ord) = &ord {
            host.emit(Equation::new(
                ord.elements[i].clone(),
                Expr::Call {
                    function: format!("residue$ord${form}${i}${num_len}${n}"),
                    args: entries.clone(),
                },
                source,
            ))?;
        }
    }
    host.emit(Equation::new(
        output_at(outputs, array_outputs)?.clone(),
        Expr::Call {
            function: format!("residue$k${form}${num_len}${n}"),
            args: entries,
        },
        source,
    ))
}

/// `CALL errorconst(num, den : Kp, Kv, Ka)` — the position, velocity and
/// acceleration static error constants of an open loop. Port of
/// `flattenErrorConst`, which serialises the numerator **unpadded**.
fn flatten_error_const<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 2 || outputs.len() != 3 {
        return Err(parse_err(
            "errorconst expects 2 inputs (num, den) and 3 scalar outputs (Kp, Kv, Ka), \
             e.g. CALL errorconst(num[1:2], den[1:3] : Kp, Kv, Ka)",
        ));
    }
    let num = host.vector_info(&inputs[0])?;
    let den = host.vector_info(&inputs[1])?;
    let (num_len, den_len) = (num.size, den.size);
    let mut entries = num.elements;
    entries.extend(den.elements);

    for (k, part) in ["kp", "kv", "ka"].iter().enumerate() {
        host.emit(Equation::new(
            outputs[k].clone(),
            Expr::Call {
                function: format!("errorconst${part}${num_len}${den_len}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    Ok(())
}

/// `CALL mason(G, source, sink : T)` — the overall transmittance of a
/// signal-flow graph by Mason's gain formula. Port of `flattenMason`.
fn flatten_mason<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 3 || outputs.len() != 1 {
        return Err(parse_err(
            "mason expects 3 inputs (G, source, sink) and 1 output (T), \
             e.g. CALL mason(G[1:4,1:4], 1, 4 : T)",
        ));
    }
    let g = host.matrix_info(&inputs[0])?;
    if g.rows != g.cols {
        return Err(parse_err("mason: G must be a square node-gain matrix"));
    }
    let n = g.rows;
    let src_node = scalar_element(host, &inputs[1])?;
    let sink_node = scalar_element(host, &inputs[2])?;

    let mut entries = g.entries();
    entries.push(src_node);
    entries.push(sink_node);

    host.emit(Equation::new(
        outputs[0].clone(),
        Expr::Call {
            function: format!("mason${n}"),
            args: entries,
        },
        source,
    ))
}

/// `CALL c2d(num, den, Ts [, method$] : numz, denz)` and its `d2c` inverse.
/// Port of `flattenDiscretize`: the method is encoded into the synthetic name,
/// the coefficients and `Ts` into its arguments.
fn flatten_discretize<H: Host + ?Sized>(
    host: &mut H,
    name: &str,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if (inputs.len() != 3 && inputs.len() != 4) || outputs.len() != 2 {
        return Err(parse_err(format!(
            "{name} expects 3 inputs (num, den, Ts) or 4 inputs (num, den, Ts, method$) \
             and 2 outputs (numz, denz), e.g. CALL {name}(num[1:2], den[1:2], Ts, 'tustin' : \
             numz[1:2], denz[1:2])"
        )));
    }
    let mut method = "tustin".to_string();
    if inputs.len() == 4 {
        let Expr::Str(method_raw) = &inputs[3] else {
            return Err(parse_err(format!(
                "{name}: the fourth argument must be a quoted method, 'tustin' or 'zoh'"
            )));
        };
        method = method_raw.to_lowercase();
        if method != "tustin" && method != "bilinear" && method != "zoh" {
            return Err(parse_err(format!(
                "{name}: method must be 'tustin' or 'zoh' (got '{method_raw}')"
            )));
        }
        if name == "d2c" && method == "zoh" {
            return Err(parse_err("d2c: only the 'tustin' method is supported"));
        }
    }
    let num = host.vector_info(&inputs[0])?;
    let den = host.vector_info(&inputs[1])?;
    let num_padded = pad_numerator(name, &num, &den)?;
    let ts = scalar_element(host, &inputs[2])?;

    let out_len = den.size;
    let numz = host.vector_info(&outputs[0])?;
    let denz = host.vector_info(&outputs[1])?;
    if numz.size != out_len || denz.size != out_len {
        return Err(parse_err(format!(
            "{name}: outputs numz and denz must have the same length as den = {out_len}"
        )));
    }
    register_slice_shape(host, &outputs[0], out_len, 1);
    register_slice_shape(host, &outputs[1], out_len, 1);

    let mut entries = num_padded;
    entries.extend(den.elements);
    entries.push(ts);

    let len = out_len;
    host.reserve(2 * out_len)?;
    for i in 0..out_len {
        host.emit(Equation::new(
            numz.elements[i].clone(),
            Expr::Call {
                function: format!("{name}$num${method}${i}${len}"),
                args: entries.clone(),
            },
            source,
        ))?;
        host.emit(Equation::new(
            denz.elements[i].clone(),
            Expr::Call {
                function: format!("{name}$den${method}${i}${len}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Time response
// ---------------------------------------------------------------------------

/// Default response time grid: [`DEFAULT_TIME_POINTS`] points evenly spaced
/// over `[0, DEFAULT_TIME_FINAL]` seconds. Port of `defaultTimeGrid`.
fn default_time_grid() -> Vec<Expr> {
    let dt = DEFAULT_TIME_FINAL / (DEFAULT_TIME_POINTS - 1) as f64;
    (0..DEFAULT_TIME_POINTS)
        .map(|i| Expr::num(i as f64 * dt))
        .collect()
}

/// `CALL step(...)` / `CALL impulse(...)`. The model is `(num, den)` or
/// `(A, B, C, D)`, optionally followed by a time vector; when it is omitted the
/// response is sampled on the default grid, which an optional second output
/// captures (`[y, t] = step(num, den)`). Port of `flattenTimeResponse`.
fn flatten_time_response<H: Host + ?Sized>(
    host: &mut H,
    name: &str,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    let state_space = inputs.len() >= 4;
    let model_inputs = if state_space { 4 } else { 2 };
    let has_time = inputs.len() == model_inputs + 1;
    if (inputs.len() != model_inputs && !has_time)
        || outputs.is_empty()
        || outputs.len() > 2
        || (outputs.len() == 2 && has_time)
    {
        return Err(parse_err(format!(
            "{name} expects (num, den) or (A, B, C, D), optionally followed by a time vector t, \
             and 1 output y (or [y, t] to capture the auto-generated time grid), \
             e.g. CALL {name}(num, den : y[1:{DEFAULT_TIME_POINTS}])"
        )));
    }

    let time_samples = if has_time {
        host.vector_info(&inputs[inputs.len() - 1])?.elements
    } else {
        default_time_grid()
    };
    let n_pts = time_samples.len();

    let y = host.vector_info(&outputs[0])?;
    if y.size != n_pts {
        return Err(parse_err(format!(
            "{name}: output y must have the same size N as t = {n_pts}"
        )));
    }

    let mut entries = if state_space {
        ss_entries(host, name, inputs)?
    } else {
        tf_entries(host, name, inputs)?
    };
    entries.extend(time_samples.iter().cloned());

    register_slice_shape(host, &outputs[0], n_pts, 1);

    // Tag with the *with-time* input count (3 = TF, 5 = SS) so the auto-grid
    // case reuses that evaluator layout.
    let tag = model_inputs + 1;
    host.reserve(n_pts)?;
    for i in 0..n_pts {
        host.emit(Equation::new(
            y.elements[i].clone(),
            Expr::Call {
                function: format!("{name}${i}${tag}${n_pts}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }

    if outputs.len() == 2 {
        let tout = host.vector_info(&outputs[1])?;
        if tout.size != n_pts {
            return Err(parse_err(format!(
                "{name}: time output must have length N = {n_pts}"
            )));
        }
        register_slice_shape(host, &outputs[1], n_pts, 1);
        host.reserve(n_pts)?;
        for (slot, sample) in tout.elements.iter().zip(&time_samples) {
            host.emit(Equation::new(slot.clone(), sample.clone(), source))?;
        }
    }
    Ok(())
}

/// `CALL lsim(num, den, u, t : y)` or the 6-input state-space form. Port of
/// `flattenLsim`.
fn flatten_lsim<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if (inputs.len() != 4 && inputs.len() != 6) || outputs.len() != 1 {
        return Err(parse_err(
            "lsim expects 4 inputs (num, den, u, t) or 6 inputs (A, B, C, D, u, t) and \
             1 output (y), e.g. CALL lsim(num, den, u, t : y[1:50])",
        ));
    }
    let num_inputs = inputs.len();
    let input = host.vector_info(&inputs[num_inputs - 2])?;
    let time = host.vector_info(&inputs[num_inputs - 1])?;
    let n_pts = time.size;
    if input.size != n_pts {
        return Err(parse_err(format!(
            "lsim: input u and time t must have the same size N = {n_pts}"
        )));
    }
    let y = host.vector_info(&outputs[0])?;
    if y.size != n_pts {
        return Err(parse_err(format!(
            "lsim: output y must have the same size N as t = {n_pts}"
        )));
    }

    let mut entries = if num_inputs == 4 {
        tf_entries(host, "lsim", inputs)?
    } else {
        ss_entries(host, "lsim", inputs)?
    };
    entries.extend(input.elements);
    entries.extend(time.elements);

    register_slice_shape(host, &outputs[0], n_pts, 1);
    host.reserve(n_pts)?;
    for i in 0..n_pts {
        host.emit(Equation::new(
            y.elements[i].clone(),
            Expr::Call {
                function: format!("lsim${i}${num_inputs}${n_pts}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    Ok(())
}

/// `CALL stepinfo(t, y : Tr, Tp, Ts, OS)`. Port of `flattenStepInfo`.
fn flatten_stepinfo<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 2 || outputs.len() != 4 {
        return Err(parse_err(
            "stepinfo expects 2 inputs (t, y) and 4 scalar outputs (Tr, Tp, Ts, OS), \
             e.g. CALL stepinfo(t[1:50], y[1:50] : Tr, Tp, Ts, OS)",
        ));
    }
    let t = host.vector_info(&inputs[0])?;
    let n_pts = t.size;
    let y = host.vector_info(&inputs[1])?;
    if y.size != n_pts {
        return Err(parse_err(format!(
            "stepinfo: inputs t and y must have the same length (got t: {n_pts}, y: {})",
            y.size
        )));
    }
    let mut entries = t.elements;
    entries.extend(y.elements);

    for (k, part) in ["tr", "tp", "ts", "os"].iter().enumerate() {
        host.emit(Equation::new(
            outputs[k].clone(),
            Expr::Call {
                function: format!("stepinfo${part}${n_pts}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    Ok(())
}

/// `CALL pade(Td, order : num_delay, den_delay)`. Port of `flattenPade` — the
/// two inputs are serialised **raw**, exactly as the Java does.
fn flatten_pade<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 2 || outputs.len() != 2 {
        return Err(parse_err(
            "pade expects 2 inputs (Td, order) and 2 vector outputs (num_delay, den_delay), \
             e.g. CALL pade(Td, order : num_delay[1:3], den_delay[1:3])",
        ));
    }
    let order = host.const_index(&inputs[1])?;
    if order < 1 {
        return Err(parse_err("pade: order must be >= 1"));
    }
    let order = order as usize;
    let m = order + 1;
    let num = host.vector_info(&outputs[0])?;
    let den = host.vector_info(&outputs[1])?;
    if num.size != m || den.size != m {
        return Err(parse_err(format!(
            "pade: outputs num_delay and den_delay must have size {m} (order + 1) for a \
             Padé approximation of order {order}"
        )));
    }
    register_slice_shape(host, &outputs[0], m, 1);
    register_slice_shape(host, &outputs[1], m, 1);

    let entries = vec![inputs[0].clone(), inputs[1].clone()];
    host.reserve(2 * m)?;
    for i in 0..m {
        host.emit(Equation::new(
            num.elements[i].clone(),
            Expr::Call {
                function: format!("pade$num${i}${order}"),
                args: entries.clone(),
            },
            source,
        ))?;
        host.emit(Equation::new(
            den.elements[i].clone(),
            Expr::Call {
                function: format!("pade$den${i}${order}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    Ok(())
}

/// `CALL rlocus(num, den : K, cpr, cpi)`. Port of `flattenRlocus`.
fn flatten_rlocus<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 2 || outputs.len() != 3 {
        return Err(parse_err(
            "rlocus expects 2 inputs (num, den) and 3 outputs (K[1:M], cpr[1:M, 1:N], \
             cpi[1:M, 1:N]), e.g. CALL rlocus(num, den : K[1:100], cpr[1:100, 1:4], \
             cpi[1:100, 1:4])",
        ));
    }
    let num = host.vector_info(&inputs[0])?;
    let den = host.vector_info(&inputs[1])?;
    if num.size > den.size {
        return Err(parse_err(
            "rlocus: system must be proper (numerator order <= denominator order)",
        ));
    }
    let k_out = host.vector_info(&outputs[0])?;
    let m = k_out.size;

    let cpr = host.matrix_info(&outputs[1])?;
    let cpi = host.matrix_info(&outputs[2])?;

    let n = den.size.saturating_sub(1); // system order
    if cpr.rows != m || cpr.cols != n {
        return Err(parse_err(format!(
            "rlocus: cpr must be a matrix of size {m}x{n} (got {}x{})",
            cpr.rows, cpr.cols
        )));
    }
    if cpi.rows != m || cpi.cols != n {
        return Err(parse_err(format!(
            "rlocus: cpi must be a matrix of size {m}x{n} (got {}x{})",
            cpi.rows, cpi.cols
        )));
    }

    host.register_shape(&k_out.name, m, 1);
    host.register_shape(&cpr.name, m, n);
    host.register_shape(&cpi.name, m, n);

    let (num_size, den_size) = (num.size, den.size);
    let mut entries = num.elements;
    entries.extend(den.elements);
    let suffix = format!("${num_size}${den_size}${m}${n}");

    host.reserve(m + 2 * m * n)?;
    for i in 0..m {
        host.emit(Equation::new(
            k_out.elements[i].clone(),
            Expr::Call {
                function: format!("rlocus$k${i}{suffix}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    for i in 0..m {
        for j in 0..n {
            host.emit(Equation::new(
                cpr.elements[i][j].clone(),
                Expr::Call {
                    function: format!("rlocus$cpr${i}${j}{suffix}"),
                    args: entries.clone(),
                },
                source,
            ))?;
            host.emit(Equation::new(
                cpi.elements[i][j].clone(),
                Expr::Call {
                    function: format!("rlocus$cpi${i}${j}{suffix}"),
                    args: entries.clone(),
                },
                source,
            ))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Design
// ---------------------------------------------------------------------------

/// `CALL lqr(A, B, Q, R : K)` and its `dlqr` / `dare` siblings. Port of
/// `flattenLqrLike`: `dare` returns the n×n Riccati solution, the other two an
/// m×n gain.
fn flatten_lqr_like<H: Host + ?Sized>(
    host: &mut H,
    op: &str,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 4 || outputs.len() != 1 {
        return Err(parse_err(format!(
            "{op} expects 4 inputs (A, B, Q, R) and 1 output"
        )));
    }
    let a = matrix_data(host, &inputs[0])?;
    let n = a.rows;
    if a.cols != n {
        return Err(parse_err(format!("{op}: A must be square")));
    }
    let b = matrix_data(host, &inputs[1])?;
    if b.rows != n {
        return Err(parse_err(format!("{op}: B must have n rows")));
    }
    let m = b.cols;
    let q = matrix_data(host, &inputs[2])?;
    let r = matrix_data(host, &inputs[3])?;

    let out = matrix_data(host, &outputs[0])?;
    let out_rows = if op == "dare" { n } else { m };
    let out_cols = n;

    host.register_shape(&out.name, out_rows, out_cols);

    let mut entries = a.entries();
    entries.extend(b.entries());
    entries.extend(q.entries());
    entries.extend(r.entries());

    let slots = out.out_slots();
    require_output_cells(op, slots.len(), out_rows, out_cols)?;
    host.reserve(out_rows * out_cols)?;
    let mut k = 0;
    for i in 0..out_rows {
        for j in 0..out_cols {
            host.emit(Equation::new(
                slots[k].clone(),
                Expr::Call {
                    function: format!("{op}${i}${j}${n}${m}"),
                    args: entries.clone(),
                },
                source,
            ))?;
            k += 1;
        }
    }
    Ok(())
}

/// `CALL lyap(A, Q : X)` / `CALL dlyap(A, Q : X)`. Port of `flattenLyapLike`.
fn flatten_lyap_like<H: Host + ?Sized>(
    host: &mut H,
    op: &str,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 2 || outputs.len() != 1 {
        return Err(parse_err(format!(
            "{op} expects 2 inputs (A, Q) and 1 output (X)"
        )));
    }
    let a = host.matrix_info(&inputs[0])?;
    let n = a.rows;
    if a.cols != n {
        return Err(parse_err(format!("{op}: A must be square")));
    }
    let q = host.matrix_info(&inputs[1])?;
    if q.rows != n || q.cols != n {
        return Err(parse_err(format!("{op}: Q must be square n x n")));
    }
    let out = host.matrix_info(&outputs[0])?;
    host.register_shape(&out.name, n, n);

    let mut entries = a.entries();
    entries.extend(q.entries());

    let slots = out.out_slots();
    require_output_cells(op, slots.len(), n, n)?;
    host.reserve(n * n)?;
    let mut k = 0;
    for i in 0..n {
        for j in 0..n {
            host.emit(Equation::new(
                slots[k].clone(),
                Expr::Call {
                    function: format!("{op}${i}${j}${n}"),
                    args: entries.clone(),
                },
                source,
            ))?;
            k += 1;
        }
    }
    Ok(())
}

/// `CALL ctrb(A, B : Co)` / `CALL obsv(A, C : Ob)`. Port of `flattenCtrbObsv`,
/// including its convenience transpose: an `obsv` second argument written as an
/// n-column is read as the 1×n row vector C.
fn flatten_ctrb_obsv<H: Host + ?Sized>(
    host: &mut H,
    op: &str,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 2 || outputs.len() != 1 {
        return Err(parse_err(format!("{op} expects 2 inputs and 1 output")));
    }
    let a = matrix_data(host, &inputs[0])?;
    let n = a.rows;
    if a.cols != n {
        return Err(parse_err(format!("{op}: A must be square")));
    }
    let mut b_or_c = matrix_data(host, &inputs[1])?;
    if op == "obsv" && b_or_c.cols == 1 && b_or_c.rows == n {
        let transposed = vec![(0..n).map(|i| b_or_c.elements[i][0].clone()).collect()];
        b_or_c = MatrixRef {
            name: b_or_c.name,
            rows: 1,
            cols: n,
            elements: transposed,
        };
    }

    let out_rows = if op == "ctrb" { n } else { n * b_or_c.rows };
    let out_cols = if op == "ctrb" { n * b_or_c.cols } else { n };

    let out = matrix_data(host, &outputs[0])?;
    host.register_shape(&out.name, out_rows, out_cols);

    let (r, cols) = (b_or_c.rows, b_or_c.cols);
    let mut entries = a.entries();
    entries.extend(b_or_c.entries());

    let slots = out.out_slots();
    require_output_cells(op, slots.len(), out_rows, out_cols)?;
    host.reserve(out_rows * out_cols)?;
    let mut k = 0;
    for i in 0..out_rows {
        for j in 0..out_cols {
            host.emit(Equation::new(
                slots[k].clone(),
                Expr::Call {
                    function: format!("{op}${i}${j}${n}${r}${cols}"),
                    args: entries.clone(),
                },
                source,
            ))?;
            k += 1;
        }
    }
    Ok(())
}

/// `CALL place(A, B, pr, pi : K)` (and its `acker` alias): pole placement with
/// the desired closed-loop poles given as real/imag arrays. Port of
/// `flattenPlace` — note it deliberately registers **no** shape for K.
fn flatten_place<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 4 || outputs.len() != 1 {
        return Err(parse_err(
            "place expects 4 inputs (A, B, pr, pi) and 1 output (K), \
             e.g. CALL place(A[1:2,1:2], B[1:2], pr[1:2], pi[1:2] : K[1:2])",
        ));
    }
    let a = matrix_data(host, &inputs[0])?;
    let n = a.rows;
    if a.cols != n {
        return Err(parse_err(format!(
            "place: A must be square (got {}x{})",
            a.rows, a.cols
        )));
    }
    let b = matrix_data(host, &inputs[1])?;
    if b.rows != n {
        return Err(parse_err("place: B must have n rows"));
    }
    let m = b.cols;
    let pr = host.vector_info(&inputs[2])?;
    let pi = host.vector_info(&inputs[3])?;
    if pr.size != n || pi.size != n {
        return Err(parse_err(format!(
            "place: desired pole arrays pr and pi must each have length n = {n}"
        )));
    }

    let k_out = matrix_data(host, &outputs[0])?;

    let mut entries = a.entries();
    entries.extend(b.entries());
    entries.extend(pr.elements);
    entries.extend(pi.elements);

    let slots = k_out.out_slots();
    require_output_cells("place", slots.len(), m, n)?;
    host.reserve(m * n)?;
    let mut k = 0;
    for i in 0..m {
        for j in 0..n {
            host.emit(Equation::new(
                slots[k].clone(),
                Expr::Call {
                    function: format!("place${i}${j}${n}${m}"),
                    args: entries.clone(),
                },
                source,
            ))?;
            k += 1;
        }
    }
    Ok(())
}

/// `CALL lqe(A, G, C, Q, R : L)`: the continuous-time Kalman estimator gain.
/// Port of `flattenLqe`.
fn flatten_lqe<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 5 || outputs.len() != 1 {
        return Err(parse_err(
            "lqe expects 5 inputs (A, G, C, Q, R) and 1 output (L), \
             e.g. CALL lqe(A[1:2,1:2], G[1:2,1:2], C[1:1,1:2], Q[1:2,1:2], R : L[1:2,1:1])",
        ));
    }
    let a = matrix_data(host, &inputs[0])?;
    let n = a.rows;
    if a.cols != n {
        return Err(parse_err("lqe: A must be square"));
    }
    let g = matrix_data(host, &inputs[1])?;
    if g.rows != n {
        return Err(parse_err("lqe: G must have n rows"));
    }
    let gd = g.cols;
    let cm = matrix_data(host, &inputs[2])?;
    if cm.cols != n {
        return Err(parse_err("lqe: C must have n columns"));
    }
    let p = cm.rows;
    let q = matrix_data(host, &inputs[3])?;
    if q.rows != gd || q.cols != gd {
        return Err(parse_err("lqe: Q must be g x g"));
    }
    let r = matrix_data(host, &inputs[4])?;
    if r.rows != p || r.cols != p {
        return Err(parse_err("lqe: R must be p x p"));
    }

    let out = matrix_data(host, &outputs[0])?;
    host.register_shape(&out.name, n, p);

    let mut entries = a.entries();
    entries.extend(g.entries());
    entries.extend(cm.entries());
    entries.extend(q.entries());
    entries.extend(r.entries());

    let slots = out.out_slots();
    require_output_cells("lqe", slots.len(), n, p)?;
    host.reserve(n * p)?;
    let mut k = 0;
    for i in 0..n {
        for j in 0..p {
            host.emit(Equation::new(
                slots[k].clone(),
                Expr::Call {
                    function: format!("lqe${i}${j}${n}${gd}${p}"),
                    args: entries.clone(),
                },
                source,
            ))?;
            k += 1;
        }
    }
    Ok(())
}

/// `CALL gram(A, M, type$ : W)`: the controllability (`'c'`, M = B) or
/// observability (`'o'`, M = C) gramian. Port of `flattenGram`.
fn flatten_gram<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 3 || outputs.len() != 1 {
        return Err(parse_err(
            "gram expects 3 inputs (A, M, type$) and 1 output (W), \
             e.g. CALL gram(A[1:2,1:2], B[1:2,1:1], 'c' : Wc[1:2,1:2])",
        ));
    }
    let Expr::Str(type_raw) = &inputs[2] else {
        return Err(parse_err(
            "gram: the third argument must be a quoted gramian type, 'c' or 'o'",
        ));
    };
    let gram_type = type_raw.to_lowercase();
    if gram_type != "c" && gram_type != "o" {
        return Err(parse_err(format!(
            "gram: type must be 'c' or 'o' (got '{type_raw}')"
        )));
    }
    let a = matrix_data(host, &inputs[0])?;
    let n = a.rows;
    if a.cols != n {
        return Err(parse_err("gram: A must be square"));
    }
    let m = matrix_data(host, &inputs[1])?;
    if gram_type == "c" && m.rows != n {
        return Err(parse_err("gram: for type 'c', B must have n rows"));
    }
    if gram_type == "o" && m.cols != n {
        return Err(parse_err("gram: for type 'o', C must have n columns"));
    }

    let out = matrix_data(host, &outputs[0])?;
    host.register_shape(&out.name, n, n);

    let (m_rows, m_cols) = (m.rows, m.cols);
    let mut entries = a.entries();
    entries.extend(m.entries());

    let slots = out.out_slots();
    require_output_cells("gram", slots.len(), n, n)?;
    host.reserve(n * n)?;
    let mut k = 0;
    for i in 0..n {
        for j in 0..n {
            host.emit(Equation::new(
                slots[k].clone(),
                Expr::Call {
                    function: format!("gram${gram_type}${i}${j}${n}${m_rows}${m_cols}"),
                    args: entries.clone(),
                },
                source,
            ))?;
            k += 1;
        }
    }
    Ok(())
}

/// `CALL balreal(A, B, C : Ab, Bb, Cb)`: the internally-balanced realization.
/// Port of `flattenBalreal`.
fn flatten_balreal<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 3 || outputs.len() != 3 {
        return Err(parse_err(
            "balreal expects 3 inputs (A, B, C) and 3 outputs (Ab, Bb, Cb), \
             e.g. CALL balreal(A[1:2,1:2], B[1:2,1:1], C[1:1,1:2] : Ab[1:2,1:2], \
             Bb[1:2,1:1], Cb[1:1,1:2])",
        ));
    }
    let a = matrix_data(host, &inputs[0])?;
    let n = a.rows;
    if a.cols != n {
        return Err(parse_err("balreal: A must be square"));
    }
    let b = matrix_data(host, &inputs[1])?;
    if b.rows != n {
        return Err(parse_err("balreal: B must have n rows"));
    }
    let m = b.cols;
    let cm = matrix_data(host, &inputs[2])?;
    if cm.cols != n {
        return Err(parse_err("balreal: C must have n columns"));
    }
    let p = cm.rows;

    let ab = matrix_data(host, &outputs[0])?;
    let bb = matrix_data(host, &outputs[1])?;
    let cb = matrix_data(host, &outputs[2])?;
    host.register_shape(&ab.name, n, n);
    host.register_shape(&bb.name, n, m);
    host.register_shape(&cb.name, p, n);

    let mut entries = a.entries();
    entries.extend(b.entries());
    entries.extend(cm.entries());

    host.reserve(n * n + n * m + p * n)?;
    for (tag, out, rows, cols) in [("a", &ab, n, n), ("b", &bb, n, m), ("c", &cb, p, n)] {
        let slots = out.out_slots();
        require_output_cells("balreal", slots.len(), rows, cols)?;
        let mut k = 0;
        for i in 0..rows {
            for j in 0..cols {
                host.emit(Equation::new(
                    slots[k].clone(),
                    Expr::Call {
                        function: format!("balreal${tag}${i}${j}${n}${m}${p}"),
                        args: entries.clone(),
                    },
                    source,
                ))?;
                k += 1;
            }
        }
    }
    Ok(())
}

/// `CALL pidtune(num, den, type$, wc : Kp, Ki, Kd)`: loop-shaping PID design
/// for a SISO plant with a gain crossover at `wc`. Port of `flattenPidtune`.
fn flatten_pidtune<H: Host + ?Sized>(
    host: &mut H,
    inputs: &[Expr],
    outputs: &[Expr],
    source: &str,
) -> Result<()> {
    if inputs.len() != 4 || outputs.len() != 3 {
        return Err(parse_err(
            "pidtune expects 4 inputs (num, den, type$, wc) and 3 outputs (Kp, Ki, Kd), \
             e.g. CALL pidtune(num, den, 'PID', wc : Kp, Ki, Kd)",
        ));
    }
    let Expr::Str(type_raw) = &inputs[2] else {
        return Err(parse_err(
            "pidtune: the third argument must be a quoted controller type, \
             one of 'P', 'PI', or 'PID'",
        ));
    };
    let pid_type = type_raw.to_lowercase();
    if pid_type != "p" && pid_type != "pi" && pid_type != "pid" {
        return Err(parse_err(format!(
            "pidtune: controller type must be 'P', 'PI', or 'PID' (got '{type_raw}')"
        )));
    }
    let num = host.vector_info(&inputs[0])?;
    let den = host.vector_info(&inputs[1])?;
    let num_padded = pad_numerator("pidtune", &num, &den)?;
    let wc = scalar_element(host, &inputs[3])?;

    let mut entries = num_padded;
    entries.extend(den.elements);
    entries.push(wc);

    for (k, part) in ["kp", "ki", "kd"].iter().enumerate() {
        host.emit(Equation::new(
            outputs[k].clone(),
            Expr::Call {
                function: format!("pidtune${part}${pid_type}"),
                args: entries.clone(),
            },
            source,
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A [`Host`] over literal element variables: `A[1:2,1:2]` resolves to
    /// `a[1,1] … a[2,2]` exactly as `expand::parse_matrix_info` does, so the
    /// emitted equations are byte-comparable with the Java's.
    #[derive(Default)]
    struct TestHost {
        out: Vec<Equation>,
        shapes: Vec<(String, usize, usize)>,
    }

    fn round(v: f64) -> i64 {
        libm::floor(v + 0.5) as i64
    }

    fn const_of(e: &Expr) -> Option<f64> {
        match e {
            Expr::Num { value, .. } => Some(*value),
            Expr::Neg(inner) => const_of(inner).map(|v| -v),
            _ => None,
        }
    }

    impl Shapes for TestHost {
        fn matrix_info(&self, expr: &Expr) -> Result<MatrixRef> {
            let Expr::ArrayAccess { name, indices } = expr else {
                return Err(parse_err("Expected matrix array access: e.g. A[1:3, 1:3]"));
            };
            if indices.len() != 2 {
                return Err(parse_err(format!(
                    "Matrix must have exactly 2 dimensions: {name}"
                )));
            }
            let (r0, r1) = (&indices[0], &indices[1]);
            let (Expr::Range { start: rs, end: re }, Expr::Range { start: cs, end: ce }) = (r0, r1)
            else {
                return Err(parse_err("Matrix indices must specify ranges"));
            };
            let (rs, re) = (self.const_index(rs)?, self.const_index(re)?);
            let (cs, ce) = (self.const_index(cs)?, self.const_index(ce)?);
            let rows = ((re - rs).abs() + 1) as usize;
            let cols = ((ce - cs).abs() + 1) as usize;
            let rd = if rs <= re { 1 } else { -1 };
            let cd = if cs <= ce { 1 } else { -1 };
            let elements = (0..rows)
                .map(|i| {
                    (0..cols)
                        .map(|j| {
                            Expr::Var(format!(
                                "{name}[{},{}]",
                                rs + i as i64 * rd,
                                cs + j as i64 * cd
                            ))
                        })
                        .collect()
                })
                .collect();
            Ok(MatrixRef {
                name: name.clone(),
                rows,
                cols,
                elements,
            })
        }

        fn vector_info(&self, expr: &Expr) -> Result<VectorRef> {
            let Expr::ArrayAccess { name, indices } = expr else {
                return Err(parse_err("Expected vector array access: e.g. v[1:3]"));
            };
            if indices.len() != 1 {
                return Err(parse_err(format!(
                    "Vector must have exactly 1 dimension: {name}"
                )));
            }
            let Expr::Range { start, end } = &indices[0] else {
                return Err(parse_err("Vector index must specify a range: e.g. v[1:3]"));
            };
            let (s, e) = (self.const_index(start)?, self.const_index(end)?);
            let size = ((e - s).abs() + 1) as usize;
            let dir = if s <= e { 1 } else { -1 };
            Ok(VectorRef {
                name: name.clone(),
                size,
                elements: (0..size)
                    .map(|i| Expr::Var(format!("{name}[{}]", s + i as i64 * dir)))
                    .collect(),
            })
        }

        fn expand(&self, expr: &Expr) -> Result<Expr> {
            Ok(expr.clone())
        }

        fn const_index(&self, expr: &Expr) -> Result<i64> {
            const_of(expr)
                .map(round)
                .ok_or_else(|| parse_err("Array index expression cannot be evaluated"))
        }
    }

    impl Host for TestHost {
        fn register_shape(&mut self, name: &str, rows: usize, cols: usize) {
            self.shapes.push((name.to_string(), rows, cols));
        }
        fn emit(&mut self, equation: Equation) -> Result<()> {
            self.out.push(equation);
            Ok(())
        }
        fn reserve(&self, _planned: usize) -> Result<()> {
            Ok(())
        }
    }

    fn vec_ref(name: &str, size: usize) -> Expr {
        Expr::ArrayAccess {
            name: name.to_string(),
            indices: vec![range_one_to(size)],
        }
    }

    fn mat_ref(name: &str, rows: usize, cols: usize) -> Expr {
        Expr::ArrayAccess {
            name: name.to_string(),
            indices: vec![range_one_to(rows), range_one_to(cols)],
        }
    }

    fn names(host: &TestHost) -> Vec<String> {
        host.out
            .iter()
            .map(|eq| match (&eq.lhs, &eq.rhs) {
                (Expr::Var(lhs), Expr::Call { function, .. }) => format!("{lhs} = {function}"),
                (Expr::Var(lhs), rhs) => format!("{lhs} = {rhs:?}"),
                _ => "?".to_string(),
            })
            .collect()
    }

    fn args_of(host: &TestHost, k: usize) -> Vec<String> {
        match &host.out[k].rhs {
            Expr::Call { args, .. } => args
                .iter()
                .map(|a| match a {
                    Expr::Var(v) => v.clone(),
                    Expr::Num { value, .. } => format!("{value}"),
                    other => format!("{other:?}"),
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn ss2tf_serialises_a_then_b_then_c_then_d() {
        let mut host = TestHost::default();
        let inputs = [
            mat_ref("a", 2, 2),
            vec_ref("b", 2),
            vec_ref("c", 2),
            Expr::Var("d".into()),
        ];
        let outputs = [vec_ref("num", 3), vec_ref("den", 3)];
        flatten(&mut host, "ss2tf", &inputs, &outputs, "src").unwrap();
        assert_eq!(
            args_of(&host, 0),
            ["a[1,1]", "a[1,2]", "a[2,1]", "a[2,2]", "b[1]", "b[2]", "c[1]", "c[2]", "d"]
        );
        assert_eq!(
            names(&host),
            [
                "num[1] = ss2tf$num$0$2",
                "den[1] = ss2tf$den$0$2",
                "num[2] = ss2tf$num$1$2",
                "den[2] = ss2tf$den$1$2",
                "num[3] = ss2tf$num$2$2",
                "den[3] = ss2tf$den$2$2",
            ]
        );
    }

    #[test]
    fn ss2tf_rejects_wrong_output_length() {
        let mut host = TestHost::default();
        let inputs = [
            mat_ref("a", 2, 2),
            vec_ref("b", 2),
            vec_ref("c", 2),
            Expr::Var("d".into()),
        ];
        let outputs = [vec_ref("num", 2), vec_ref("den", 2)];
        let err = flatten(&mut host, "ss2tf", &inputs, &outputs, "src").unwrap_err();
        assert!(
            err.to_string().contains("length n+1 = 3"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn tf2ss_pads_a_short_numerator_with_leading_zeros() {
        let mut host = TestHost::default();
        let inputs = [vec_ref("num", 1), vec_ref("den", 3)];
        let outputs = [
            mat_ref("aa", 2, 2),
            vec_ref("bb", 2),
            vec_ref("cc", 2),
            Expr::Var("dd".into()),
        ];
        flatten(&mut host, "tf2ss", &inputs, &outputs, "src").unwrap();
        assert_eq!(
            args_of(&host, 0),
            ["0", "0", "num[1]", "den[1]", "den[2]", "den[3]"]
        );
        // n*n + 2n + 1 equations for n = 2.
        assert_eq!(host.out.len(), 4 + 4 + 1);
    }

    #[test]
    fn improper_transfer_function_is_refused() {
        let mut host = TestHost::default();
        let inputs = [vec_ref("num", 4), vec_ref("den", 3)];
        let outputs = [
            mat_ref("aa", 2, 2),
            vec_ref("bb", 2),
            vec_ref("cc", 2),
            Expr::Var("dd".into()),
        ];
        let err = flatten(&mut host, "tf2ss", &inputs, &outputs, "src").unwrap_err();
        assert!(
            err.to_string()
                .contains("tf2ss: numerator is longer than the denominator"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn series_and_parallel_share_the_tf_combine_layout() {
        for op in ["series", "parallel"] {
            let mut host = TestHost::default();
            let inputs = [
                vec_ref("n1", 2),
                vec_ref("d1", 2),
                vec_ref("n2", 2),
                vec_ref("d2", 2),
            ];
            let outputs = [vec_ref("num", 3), vec_ref("den", 3)];
            flatten(&mut host, op, &inputs, &outputs, "src").unwrap();
            assert_eq!(
                args_of(&host, 0),
                ["n1[1]", "n1[2]", "d1[1]", "d1[2]", "n2[1]", "n2[2]", "d2[1]", "d2[2]"]
            );
            assert_eq!(names(&host)[0], format!("num[1] = {op}$num$0$2$2"));
            assert_eq!(host.out.len(), 6);
        }
    }

    #[test]
    fn feedback_appends_a_default_sign_of_one() {
        let mut host = TestHost::default();
        let inputs = [
            vec_ref("n1", 2),
            vec_ref("d1", 2),
            vec_ref("n2", 1),
            vec_ref("d2", 1),
        ];
        let outputs = [vec_ref("num", 2), vec_ref("den", 2)];
        flatten(&mut host, "feedback", &inputs, &outputs, "src").unwrap();
        assert_eq!(args_of(&host, 0).last().unwrap(), "1");
        assert_eq!(names(&host)[0], "num[1] = feedback$num$0$2$1");
    }

    #[test]
    fn state_space_series_takes_q2_and_parallel_takes_q1() {
        // A1 2x2, B1 2x1, C1 1x2, D1 1x1; A2 1x1, B2 1x1, C2 2x1, D2 2x1
        // -> series q_out = q2 = 2, parallel q_out = q1 = 1.
        let inputs = [
            mat_ref("a1", 2, 2),
            mat_ref("b1", 2, 1),
            mat_ref("c1", 1, 2),
            mat_ref("d1", 1, 1),
            mat_ref("a2", 1, 1),
            mat_ref("b2", 1, 1),
            mat_ref("c2", 2, 1),
            mat_ref("d2", 2, 1),
        ];
        for (op, want_q) in [("series", 2usize), ("parallel", 1usize)] {
            let mut host = TestHost::default();
            let outputs = [
                mat_ref("ao", 3, 3),
                mat_ref("bo", 3, 1),
                mat_ref("co", want_q, 3),
                mat_ref("do", want_q, 1),
            ];
            flatten(&mut host, op, &inputs, &outputs, "src").unwrap();
            let cn = host
                .shapes
                .iter()
                .find(|(n, _, _)| n == "co")
                .expect("C shape registered");
            assert_eq!((cn.1, cn.2), (want_q, 3), "{op}");
            assert!(names(&host)[0].starts_with(&format!("ao[1,1] = ss_{op}$a$0$0$2$1$1$1$1$2")));
        }
    }

    #[test]
    fn step_without_a_time_vector_uses_the_default_grid() {
        let mut host = TestHost::default();
        let inputs = [vec_ref("num", 2), vec_ref("den", 2)];
        let outputs = [vec_ref("y", DEFAULT_TIME_POINTS)];
        flatten(&mut host, "step", &inputs, &outputs, "src").unwrap();
        assert_eq!(host.out.len(), DEFAULT_TIME_POINTS);
        assert_eq!(names(&host)[0], "y[1] = step$0$3$50");
        // Model entries first, then the 50 samples of the default grid.
        let args = args_of(&host, 0);
        assert_eq!(args.len(), 4 + DEFAULT_TIME_POINTS);
        assert_eq!(args[4], "0");
        assert_eq!(args[4 + DEFAULT_TIME_POINTS - 1], "10");
    }

    #[test]
    fn step_captures_the_generated_grid_as_a_second_output() {
        let mut host = TestHost::default();
        let inputs = [vec_ref("num", 2), vec_ref("den", 2)];
        let outputs = [
            vec_ref("y", DEFAULT_TIME_POINTS),
            vec_ref("t", DEFAULT_TIME_POINTS),
        ];
        flatten(&mut host, "step", &inputs, &outputs, "src").unwrap();
        assert_eq!(host.out.len(), 2 * DEFAULT_TIME_POINTS);
        let last = &host.out[2 * DEFAULT_TIME_POINTS - 1];
        assert_eq!(last.lhs, Expr::Var("t[50]".into()));
        assert_eq!(last.rhs, Expr::num(10.0));
    }

    #[test]
    fn step_refuses_a_time_output_alongside_an_explicit_grid() {
        let mut host = TestHost::default();
        let inputs = [vec_ref("num", 2), vec_ref("den", 2), vec_ref("tin", 5)];
        let outputs = [vec_ref("y", 5), vec_ref("t", 5)];
        let err = flatten(&mut host, "step", &inputs, &outputs, "src").unwrap_err();
        assert!(err.to_string().contains("step expects"), "{err}");
    }

    #[test]
    fn state_space_step_tags_five_inputs() {
        let mut host = TestHost::default();
        let inputs = [
            mat_ref("a", 2, 2),
            vec_ref("b", 2),
            vec_ref("c", 2),
            Expr::Var("d".into()),
            vec_ref("t", 4),
        ];
        let outputs = [vec_ref("y", 4)];
        flatten(&mut host, "impulse", &inputs, &outputs, "src").unwrap();
        assert_eq!(names(&host)[0], "y[1] = impulse$0$5$4");
        assert_eq!(args_of(&host, 0).len(), 4 + 2 + 2 + 1 + 4);
    }

    #[test]
    fn lqr_writes_an_m_by_n_gain_and_dare_an_n_by_n_solution() {
        let inputs = [
            mat_ref("a", 2, 2),
            vec_ref("b", 2),
            mat_ref("q", 2, 2),
            Expr::Var("r".into()),
        ];
        let mut host = TestHost::default();
        flatten(&mut host, "lqr", &inputs, &[vec_ref("k", 2)], "src").unwrap();
        assert_eq!(names(&host), ["k[1] = lqr$0$0$2$1", "k[2] = lqr$0$1$2$1"]);
        assert_eq!(host.shapes, [("k".to_string(), 1, 2)]);

        let mut host = TestHost::default();
        flatten(&mut host, "dare", &inputs, &[mat_ref("x", 2, 2)], "src").unwrap();
        assert_eq!(host.out.len(), 4);
        assert_eq!(host.shapes, [("x".to_string(), 2, 2)]);
    }

    #[test]
    fn lqr_reports_an_output_too_small_instead_of_panicking() {
        let inputs = [
            mat_ref("a", 2, 2),
            vec_ref("b", 2),
            mat_ref("q", 2, 2),
            Expr::Var("r".into()),
        ];
        let mut host = TestHost::default();
        let err = flatten(&mut host, "lqr", &inputs, &[vec_ref("k", 1)], "src").unwrap_err();
        assert!(
            err.to_string().contains("room for 1x2"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn obsv_transposes_a_column_c_and_sizes_the_output() {
        let mut host = TestHost::default();
        let inputs = [mat_ref("a", 2, 2), vec_ref("c", 2)];
        flatten(&mut host, "obsv", &inputs, &[mat_ref("ob", 2, 2)], "src").unwrap();
        assert_eq!(host.shapes, [("ob".to_string(), 2, 2)]);
        assert_eq!(names(&host)[0], "ob[1,1] = obsv$0$0$2$1$2");

        let mut host = TestHost::default();
        let inputs = [mat_ref("a", 2, 2), vec_ref("b", 2)];
        flatten(&mut host, "ctrb", &inputs, &[mat_ref("co", 2, 2)], "src").unwrap();
        assert_eq!(names(&host)[0], "co[1,1] = ctrb$0$0$2$2$1");
    }

    #[test]
    fn residue_switches_form_tag_on_the_order_output() {
        let inputs = [vec_ref("num", 1), vec_ref("den", 3)];
        let mut host = TestHost::default();
        let outputs = [
            vec_ref("rr", 2),
            vec_ref("ri", 2),
            vec_ref("pr", 2),
            vec_ref("pi", 2),
            Expr::Var("kk".into()),
        ];
        flatten(&mut host, "residue", &inputs, &outputs, "src").unwrap();
        assert_eq!(names(&host)[0], "rr[1] = residue$rr$s$0$1$2");
        assert_eq!(names(&host).last().unwrap(), "kk = residue$k$s$1$2");

        let mut host = TestHost::default();
        let outputs = [
            vec_ref("rr", 2),
            vec_ref("ri", 2),
            vec_ref("pr", 2),
            vec_ref("pi", 2),
            vec_ref("ord", 2),
            Expr::Var("kk".into()),
        ];
        flatten(&mut host, "residue", &inputs, &outputs, "src").unwrap();
        assert_eq!(names(&host)[4], "ord[1] = residue$ord$o$0$1$2");
        assert_eq!(names(&host).last().unwrap(), "kk = residue$k$o$1$2");
    }

    #[test]
    fn discretize_encodes_the_method_and_rejects_zoh_for_d2c() {
        let inputs = [
            vec_ref("num", 2),
            vec_ref("den", 2),
            Expr::Var("ts".into()),
            Expr::Str("ZOH".into()),
        ];
        let mut host = TestHost::default();
        let outputs = [vec_ref("nz", 2), vec_ref("dz", 2)];
        flatten(&mut host, "c2d", &inputs, &outputs, "src").unwrap();
        assert_eq!(names(&host)[0], "nz[1] = c2d$num$zoh$0$2");

        let mut host = TestHost::default();
        let err = flatten(&mut host, "d2c", &inputs, &outputs, "src").unwrap_err();
        assert!(
            err.to_string().contains("only the 'tustin' method"),
            "{err}"
        );
    }

    #[test]
    fn pade_serialises_its_inputs_unexpanded() {
        let mut host = TestHost::default();
        let inputs = [Expr::Var("td".into()), Expr::num(2.0)];
        let outputs = [vec_ref("nd", 3), vec_ref("dd", 3)];
        flatten(&mut host, "pade", &inputs, &outputs, "src").unwrap();
        assert_eq!(args_of(&host, 0), ["td", "2"]);
        assert_eq!(names(&host)[0], "nd[1] = pade$num$0$2");
        assert_eq!(host.out.len(), 6);
    }

    #[test]
    fn gram_and_pidtune_encode_their_string_option_in_the_name() {
        let mut host = TestHost::default();
        let inputs = [
            mat_ref("a", 2, 2),
            mat_ref("b", 2, 1),
            Expr::Str("C".into()),
        ];
        flatten(&mut host, "gram", &inputs, &[mat_ref("w", 2, 2)], "src").unwrap();
        assert_eq!(names(&host)[0], "w[1,1] = gram$c$0$0$2$2$1");

        let mut host = TestHost::default();
        let inputs = [
            vec_ref("num", 1),
            vec_ref("den", 3),
            Expr::Str("PID".into()),
            Expr::Var("wc".into()),
        ];
        let outputs = [
            Expr::Var("kp".into()),
            Expr::Var("ki".into()),
            Expr::Var("kd".into()),
        ];
        flatten(&mut host, "pidtune", &inputs, &outputs, "src").unwrap();
        assert_eq!(
            names(&host),
            [
                "kp = pidtune$kp$pid",
                "ki = pidtune$ki$pid",
                "kd = pidtune$kd$pid",
            ]
        );
        // The numerator is padded to the denominator length before serialising.
        assert_eq!(
            args_of(&host, 0),
            ["0", "0", "num[1]", "den[1]", "den[2]", "den[3]", "wc"]
        );
    }

    #[test]
    fn errorconst_serialises_the_numerator_unpadded() {
        let mut host = TestHost::default();
        let inputs = [vec_ref("num", 1), vec_ref("den", 3)];
        let outputs = [
            Expr::Var("kp".into()),
            Expr::Var("kv".into()),
            Expr::Var("ka".into()),
        ];
        flatten(&mut host, "errorconst", &inputs, &outputs, "src").unwrap();
        assert_eq!(names(&host)[0], "kp = errorconst$kp$1$3");
        assert_eq!(args_of(&host, 0), ["num[1]", "den[1]", "den[2]", "den[3]"]);
    }

    #[test]
    fn margin_and_routh_and_stepinfo_emit_scalar_outputs() {
        let mut host = TestHost::default();
        let inputs = [vec_ref("num", 2), vec_ref("den", 2)];
        let outputs = [
            Expr::Var("gm".into()),
            Expr::Var("pm".into()),
            Expr::Var("wcg".into()),
            Expr::Var("wcp".into()),
        ];
        flatten(&mut host, "margin", &inputs, &outputs, "src").unwrap();
        assert_eq!(
            names(&host),
            [
                "gm = margin$gm$2",
                "pm = margin$pm$2",
                "wcg = margin$wcg$2",
                "wcp = margin$wcp$2",
            ]
        );

        let mut host = TestHost::default();
        flatten(
            &mut host,
            "routh",
            &[vec_ref("den", 4)],
            &[Expr::Var("nrhp".into()), Expr::Var("stable".into())],
            "src",
        )
        .unwrap();
        assert_eq!(
            names(&host),
            ["nrhp = routh$nrhp$4", "stable = routh$stable$4"]
        );

        let mut host = TestHost::default();
        flatten(
            &mut host,
            "stepinfo",
            &[vec_ref("t", 5), vec_ref("y", 5)],
            &[
                Expr::Var("tr".into()),
                Expr::Var("tp".into()),
                Expr::Var("ts".into()),
                Expr::Var("os".into()),
            ],
            "src",
        )
        .unwrap();
        assert_eq!(names(&host)[0], "tr = stepinfo$tr$5");
    }

    #[test]
    fn mason_appends_source_and_sink_after_the_gain_matrix() {
        let mut host = TestHost::default();
        let inputs = [mat_ref("g", 2, 2), Expr::num(1.0), Expr::num(2.0)];
        flatten(&mut host, "mason", &inputs, &[Expr::Var("t".into())], "src").unwrap();
        assert_eq!(names(&host), ["t = mason$2"]);
        assert_eq!(
            args_of(&host, 0),
            ["g[1,1]", "g[1,2]", "g[2,1]", "g[2,2]", "1", "2"]
        );
    }

    #[test]
    fn bode_nyquist_and_nichols_differ_only_in_their_output_tags() {
        let inputs = [vec_ref("num", 2), vec_ref("den", 2), vec_ref("w", 3)];
        for (op, t0, t1) in [
            ("bode", "mag", "phase"),
            ("nyquist", "real", "imag"),
            ("nichols", "mag", "phase"),
        ] {
            let mut host = TestHost::default();
            let outputs = [vec_ref("o1", 3), vec_ref("o2", 3)];
            flatten(&mut host, op, &inputs, &outputs, "src").unwrap();
            assert_eq!(names(&host)[0], format!("o1[1] = {op}${t0}$0$3$3"));
            assert_eq!(names(&host)[1], format!("o2[1] = {op}${t1}$0$3$3"));
            assert_eq!(args_of(&host, 0).len(), 4 + 3);
        }
    }

    #[test]
    fn pole_tags_its_input_count_and_zero_accepts_both_forms() {
        let mut host = TestHost::default();
        flatten(
            &mut host,
            "pole",
            &[mat_ref("a", 2, 2)],
            &[vec_ref("pr", 2), vec_ref("pi", 2)],
            "src",
        )
        .unwrap();
        assert_eq!(names(&host)[0], "pr[1] = pole$pr$0$1$2");

        let mut host = TestHost::default();
        flatten(
            &mut host,
            "pole",
            &[vec_ref("num", 1), vec_ref("den", 3)],
            &[vec_ref("pr", 2), vec_ref("pi", 2)],
            "src",
        )
        .unwrap();
        assert_eq!(names(&host)[0], "pr[1] = pole$pr$0$2$2");
        assert_eq!(
            args_of(&host, 0),
            ["0", "0", "num[1]", "den[1]", "den[2]", "den[3]"]
        );

        let mut host = TestHost::default();
        flatten(
            &mut host,
            "zero",
            &[
                mat_ref("a", 2, 2),
                vec_ref("b", 2),
                vec_ref("c", 2),
                Expr::Var("d".into()),
            ],
            &[vec_ref("zr", 1), vec_ref("zi", 1)],
            "src",
        )
        .unwrap();
        assert_eq!(names(&host)[0], "zr[1] = zero$zr$0$4$1");
    }

    #[test]
    fn auto_size_grows_bare_outputs_from_the_inputs() {
        let host = TestHost::default();
        let inputs = [vec_ref("num", 2), vec_ref("den", 2)];
        let mut outputs = [Expr::Var("y".into())];
        auto_size(&host, "step", &inputs, &mut outputs).unwrap();
        assert_eq!(outputs[0], vec_ref("y", DEFAULT_TIME_POINTS));

        let mut outputs = [Expr::Var("num".into()), Expr::Var("den".into())];
        let inputs = [
            mat_ref("a", 3, 3),
            vec_ref("b", 3),
            vec_ref("c", 3),
            Expr::Var("d".into()),
        ];
        auto_size(&host, "ss2tf", &inputs, &mut outputs).unwrap();
        assert_eq!(outputs[0], vec_ref("num", 4));
        assert_eq!(outputs[1], vec_ref("den", 4));

        // Single-input lqr sizes K as a plain n-vector, MIMO as m×n.
        let mut outputs = [Expr::Var("k".into())];
        let inputs = [
            mat_ref("a", 2, 2),
            vec_ref("b", 2),
            mat_ref("q", 2, 2),
            Expr::Var("r".into()),
        ];
        auto_size(&host, "lqr", &inputs, &mut outputs).unwrap();
        assert_eq!(outputs[0], vec_ref("k", 2));

        let mut outputs = [Expr::Var("k".into())];
        let inputs = [
            mat_ref("a", 2, 2),
            mat_ref("b", 2, 2),
            mat_ref("q", 2, 2),
            mat_ref("r", 2, 2),
        ];
        auto_size(&host, "lqr", &inputs, &mut outputs).unwrap();
        assert_eq!(outputs[0], mat_ref("k", 2, 2));

        // rlocus takes the fixed 100-point default grid.
        let mut outputs = [
            Expr::Var("kk".into()),
            Expr::Var("cpr".into()),
            Expr::Var("cpi".into()),
        ];
        let inputs = [vec_ref("num", 1), vec_ref("den", 3)];
        auto_size(&host, "rlocus", &inputs, &mut outputs).unwrap();
        assert_eq!(outputs[0], vec_ref("kk", DEFAULT_RLOCUS_POINTS));
        assert_eq!(outputs[1], mat_ref("cpr", DEFAULT_RLOCUS_POINTS, 2));
    }

    /// The flattener and [`crate::control::eval`] are two halves of one wire
    /// format. This drives one representative CALL of every shape and asserts
    /// that **every** synthetic name emitted is claimed by the evaluator — the
    /// failure mode a per-intrinsic test cannot catch is a name family that no
    /// evaluator arm recognises, which surfaces only as "not yet supported" on
    /// a document that solved in the Java.
    #[test]
    fn every_emitted_synthetic_is_claimed_by_the_evaluator() {
        let v = vec_ref;
        let m = mat_ref;
        let s = |x: &str| Expr::Var(x.to_string());
        let cases: Vec<(&str, Vec<Expr>, Vec<Expr>)> = vec![
            ("rank", vec![m("mm", 2, 2)], vec![s("r")]),
            (
                "ss2tf",
                vec![m("a", 2, 2), v("b", 2), v("c", 2), s("d")],
                vec![v("num", 3), v("den", 3)],
            ),
            (
                "ss2tfij",
                vec![
                    m("a", 2, 2),
                    m("b", 2, 2),
                    m("c", 2, 2),
                    m("dm", 2, 2),
                    Expr::num(1.0),
                    Expr::num(1.0),
                ],
                vec![v("num", 3), v("den", 3)],
            ),
            (
                "tf2ss",
                vec![v("num", 3), v("den", 3)],
                vec![m("aa", 2, 2), v("bb", 2), v("cc", 2), s("dd")],
            ),
            (
                "zp2tf",
                vec![v("zr", 1), v("zi", 1), v("pr", 2), v("pi", 2), s("kk")],
                vec![v("num", 3), v("den", 3)],
            ),
            (
                "tf2zp",
                vec![v("num", 3), v("den", 3)],
                vec![v("zr", 1), v("zi", 1), v("pr", 2), v("pi", 2), s("kk")],
            ),
            (
                "series",
                vec![v("n1", 2), v("d1", 2), v("n2", 2), v("d2", 2)],
                vec![v("num", 3), v("den", 3)],
            ),
            (
                "parallel",
                vec![v("n1", 2), v("d1", 2), v("n2", 2), v("d2", 2)],
                vec![v("num", 3), v("den", 3)],
            ),
            (
                "feedback",
                vec![v("n1", 2), v("d1", 2), v("n2", 2), v("d2", 2)],
                vec![v("num", 3), v("den", 3)],
            ),
            (
                "series",
                vec![
                    m("a1", 1, 1),
                    m("b1", 1, 1),
                    m("c1", 1, 1),
                    m("d1", 1, 1),
                    m("a2", 1, 1),
                    m("b2", 1, 1),
                    m("c2", 1, 1),
                    m("d2", 1, 1),
                ],
                vec![m("ao", 2, 2), m("bo", 2, 1), m("co", 1, 2), m("do", 1, 1)],
            ),
            (
                "parallel",
                vec![
                    m("a1", 1, 1),
                    m("b1", 1, 1),
                    m("c1", 1, 1),
                    m("d1", 1, 1),
                    m("a2", 1, 1),
                    m("b2", 1, 1),
                    m("c2", 1, 1),
                    m("d2", 1, 1),
                ],
                vec![m("ao", 2, 2), m("bo", 2, 1), m("co", 1, 2), m("do", 1, 1)],
            ),
            (
                "feedback",
                vec![
                    m("a1", 1, 1),
                    m("b1", 1, 1),
                    m("c1", 1, 1),
                    m("d1", 1, 1),
                    m("a2", 1, 1),
                    m("b2", 1, 1),
                    m("c2", 1, 1),
                    m("d2", 1, 1),
                ],
                vec![m("ao", 2, 2), m("bo", 2, 1), m("co", 1, 2), m("do", 1, 1)],
            ),
            ("pole", vec![m("a", 2, 2)], vec![v("pr", 2), v("pi", 2)]),
            (
                "pole",
                vec![v("num", 3), v("den", 3)],
                vec![v("pr", 2), v("pi", 2)],
            ),
            (
                "zero",
                vec![v("num", 3), v("den", 3)],
                vec![v("zr", 2), v("zi", 2)],
            ),
            (
                "zero",
                vec![m("a", 2, 2), v("b", 2), v("c", 2), s("d")],
                vec![v("zr", 2), v("zi", 2)],
            ),
            (
                "bode",
                vec![v("num", 2), v("den", 2), v("w", 3)],
                vec![v("o1", 3), v("o2", 3)],
            ),
            (
                "nyquist",
                vec![v("num", 2), v("den", 2), v("w", 3)],
                vec![v("o1", 3), v("o2", 3)],
            ),
            (
                "nichols",
                vec![v("num", 2), v("den", 2), v("w", 3)],
                vec![v("o1", 3), v("o2", 3)],
            ),
            (
                "margin",
                vec![v("num", 2), v("den", 2)],
                vec![s("gm"), s("pm"), s("wcg"), s("wcp")],
            ),
            ("routh", vec![v("den", 3)], vec![s("nr"), s("st")]),
            (
                "residue",
                vec![v("num", 1), v("den", 3)],
                vec![v("rr", 2), v("ri", 2), v("pr", 2), v("pi", 2), s("kk")],
            ),
            (
                "residue",
                vec![v("num", 1), v("den", 3)],
                vec![
                    v("rr", 2),
                    v("ri", 2),
                    v("pr", 2),
                    v("pi", 2),
                    v("ord", 2),
                    s("kk"),
                ],
            ),
            (
                "errorconst",
                vec![v("num", 1), v("den", 3)],
                vec![s("kp"), s("kv"), s("ka")],
            ),
            (
                "mason",
                vec![m("g", 2, 2), Expr::num(1.0), Expr::num(2.0)],
                vec![s("t")],
            ),
            (
                "c2d",
                vec![v("num", 2), v("den", 2), s("ts")],
                vec![v("nz", 2), v("dz", 2)],
            ),
            (
                "d2c",
                vec![v("num", 2), v("den", 2), s("ts")],
                vec![v("nz", 2), v("dz", 2)],
            ),
            (
                "step",
                vec![v("num", 2), v("den", 2)],
                vec![v("y", DEFAULT_TIME_POINTS)],
            ),
            (
                "impulse",
                vec![m("a", 2, 2), v("b", 2), v("c", 2), s("d"), v("t", 4)],
                vec![v("y", 4)],
            ),
            (
                "lsim",
                vec![v("num", 2), v("den", 2), v("u", 3), v("t", 3)],
                vec![v("y", 3)],
            ),
            (
                "lqr",
                vec![m("a", 2, 2), v("b", 2), m("q", 2, 2), s("r")],
                vec![v("k", 2)],
            ),
            (
                "dlqr",
                vec![m("a", 2, 2), v("b", 2), m("q", 2, 2), s("r")],
                vec![v("k", 2)],
            ),
            (
                "dare",
                vec![m("a", 2, 2), v("b", 2), m("q", 2, 2), s("r")],
                vec![m("x", 2, 2)],
            ),
            ("lyap", vec![m("a", 2, 2), m("q", 2, 2)], vec![m("x", 2, 2)]),
            (
                "dlyap",
                vec![m("a", 2, 2), m("q", 2, 2)],
                vec![m("x", 2, 2)],
            ),
            (
                "place",
                vec![m("a", 2, 2), v("b", 2), v("pr", 2), v("pi", 2)],
                vec![v("k", 2)],
            ),
            (
                "acker",
                vec![m("a", 2, 2), v("b", 2), v("pr", 2), v("pi", 2)],
                vec![v("k", 2)],
            ),
            (
                "lqe",
                vec![
                    m("a", 2, 2),
                    m("g", 2, 1),
                    m("c", 1, 2),
                    m("q", 1, 1),
                    m("r", 1, 1),
                ],
                vec![m("l", 2, 1)],
            ),
            (
                "gram",
                vec![m("a", 2, 2), m("b", 2, 1), Expr::Str("c".into())],
                vec![m("w", 2, 2)],
            ),
            (
                "balreal",
                vec![m("a", 2, 2), m("b", 2, 1), m("c", 1, 2)],
                vec![m("ab", 2, 2), m("bb", 2, 1), m("cb", 1, 2)],
            ),
            (
                "pidtune",
                vec![v("num", 1), v("den", 3), Expr::Str("pid".into()), s("wc")],
                vec![s("kp"), s("ki"), s("kd")],
            ),
            ("ctrb", vec![m("a", 2, 2), v("b", 2)], vec![m("co", 2, 2)]),
            ("obsv", vec![m("a", 2, 2), v("c", 2)], vec![m("ob", 2, 2)]),
            (
                "ss2ss",
                vec![
                    m("a", 2, 2),
                    m("b", 2, 1),
                    m("c", 1, 2),
                    m("dm", 1, 1),
                    m("p", 2, 2),
                ],
                vec![m("an", 2, 2), m("bn", 2, 1), m("cn", 1, 2), m("dn", 1, 1)],
            ),
            (
                "stepinfo",
                vec![v("t", 5), v("y", 5)],
                vec![s("tr"), s("tp"), s("ts"), s("os")],
            ),
            (
                "pade",
                vec![s("td"), Expr::num(2.0)],
                vec![v("nd", 3), v("dd", 3)],
            ),
            (
                "rlocus",
                vec![v("num", 1), v("den", 3)],
                vec![v("kk", 4), m("cpr", 4, 2), m("cpi", 4, 2)],
            ),
        ];

        let mut covered: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for (name, inputs, outputs) in &cases {
            let mut host = TestHost::default();
            flatten(&mut host, name, inputs, outputs, "src")
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            for eq in &host.out {
                if let Expr::Call { function, .. } = &eq.rhs {
                    assert!(
                        crate::control::eval::handles(function),
                        "{name} emits `{function}`, which control::eval does not claim"
                    );
                    covered.insert(function.split('$').next().unwrap_or(function).to_string());
                }
            }
        }
        // Every name in the dispatch table produced at least one synthetic
        // (`step`'s optional time output is the only equation with a non-call
        // right-hand side, and it is covered by its own test).
        assert!(
            covered.len() >= 30,
            "only {} families covered",
            covered.len()
        );
    }

    #[test]
    fn every_call_name_is_dispatched() {
        for name in CALL_NAMES {
            let mut host = TestHost::default();
            // Deliberately wrong arity: the point is that the name resolves to a
            // handler (an arity complaint) rather than falling through.
            let err = flatten(&mut host, name, &[], &[], "src").unwrap_err();
            assert!(
                !err.to_string().contains("is not a control-systems CALL"),
                "{name} is not dispatched: {err}"
            );
        }
        assert!(handles("mason"));
    }
}
