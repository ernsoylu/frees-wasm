//! Dense linear-algebra kernels for the matrix intrinsics.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/core/LinearAlgebra.java`
//! (QR, Cholesky, singular values / SVD, matrix exponential) plus the LU
//! determinant the Java `Evaluator` uses for the synthetic `det$<n>` call.
//!
//! # How the Java engine splits matrix work (mirrored here)
//!
//! `EquationParser` expands most matrix operations into **equations** at parse
//! time — `SolveLinear`/`Inverse`/backslash emit `A·x = b` / `A·A⁻¹ = I` row
//! equations, so the solver stays scalar and *no* numeric kernel is involved.
//! Only the operations without a practical equation form are evaluated
//! numerically **at solve time** from the resolved matrix entries, via
//! synthetic `$`-calls the expansion emits (`det$3`, `qr$q$0$1$2$2`, …) whose
//! arguments are the flattened row-major entries. The Java `Evaluator`
//! dispatches those names into `LinearAlgebra`; the Rust evaluator can do the
//! same through [`eval_intrinsic`].
//!
//! The Java kernels sit on Apache Commons Math. This port keeps the same
//! algorithms and conventions:
//!
//! * [`det_lu`] — Doolittle LU with partial pivoting, singularity threshold
//!   `1e-11`, determinant `0` for a singular matrix (Commons Math
//!   `LUDecomposition` defaults).
//! * [`qr_q`] / [`qr_r`] — Householder QR with the Commons Math sign choice
//!   (`a = r_kk = ∓‖x‖`, negative for a positive pivot).
//! * [`cholesky_l`] — relaxed thresholds exactly as the Java call site:
//!   relative symmetry `1.0e-9`, absolute positivity `1.0e-12`.
//! * [`expm`] — scaling-and-squaring with a [6/6] Padé approximant, a
//!   line-for-line port of the Java method (Commons Math has no `expm`).
//! * [`singular_values`] / [`svd`] — one-sided Jacobi. Values match Commons
//!   Math to solver tolerance; the **column signs** of U/V are normalised here
//!   (largest-magnitude component of each V column made positive), which is a
//!   deterministic convention but not bit-identical to Commons Math's
//!   bidiagonalisation output. Recorded as a known divergence.

// Float guards here are written `!(x > 0.0)` on purpose: the negation makes
// NaN take the reject branch, which `x <= 0.0` would not. Clippy's
// `neg_cmp_op_on_partial_ord` exists to catch the *accidental* form; here the
// NaN behaviour is the point, and it matches the Java guards being ported.
#![allow(clippy::neg_cmp_op_on_partial_ord)]
// Numerical kernels index several parallel arrays (and 2-D `a[i][j]` slices)
// by the same loop variable, mirroring the Java/Fortran sources being
// transcribed. Iterator rewrites obscure that correspondence, so the indexed
// form stays.
#![allow(clippy::needless_range_loop)]
// Truncated constants such as `0.636619772` (2/pi) are transcribed verbatim
// from the Java `Evaluator.java` / Numerical Recipes coefficient tables.
// Substituting `std::f64::consts::*` would change the value in the last digits
// and break bit-parity with the oracle these tests compare against.
#![allow(clippy::approx_constant)]

use crate::diag::{FreesError, Result};

/// A dense row-major matrix, mirroring the Java `double[][]`.
pub type Mat = Vec<Vec<f64>>;

/// Commons Math `LUDecomposition` default singularity threshold.
const LU_SINGULARITY_THRESHOLD: f64 = 1e-11;

fn err(message: impl Into<String>) -> FreesError {
    FreesError::evaluation(message)
}

/// Reshape `rows*cols` row-major entries into a matrix.
///
/// Mirrors `Evaluator.readMatrix`.
pub fn from_row_major(entries: &[f64], rows: usize, cols: usize) -> Result<Mat> {
    if entries.len() != rows * cols {
        return Err(err(format!(
            "matrix kernel expected {rows}x{cols} = {} entries, got {}",
            rows * cols,
            entries.len()
        )));
    }
    Ok((0..rows)
        .map(|i| entries[i * cols..(i + 1) * cols].to_vec())
        .collect())
}

// ---------------------------------------------------------------------------
// LU determinant (`det$<n>`)
// ---------------------------------------------------------------------------

/// Determinant via Doolittle LU with partial pivoting — the Commons Math
/// `LUDecomposition.getDeterminant()` semantics: `0.0` when a pivot falls
/// below the `1e-11` singularity threshold.
pub fn det_lu(a: &Mat) -> Result<f64> {
    let m = check_square(a, "det")?;
    let mut lu: Mat = a.clone();
    let mut even = true;
    for col in 0..m {
        // Upper part.
        for row in 0..col {
            let mut sum = lu[row][col];
            for i in 0..row {
                sum -= lu[row][i] * lu[i][col];
            }
            lu[row][col] = sum;
        }
        // Lower part, tracking the largest pivot candidate.
        let mut max = col;
        let mut largest = f64::NEG_INFINITY;
        for row in col..m {
            let mut sum = lu[row][col];
            for i in 0..col {
                sum -= lu[row][i] * lu[i][col];
            }
            lu[row][col] = sum;
            if sum.abs() > largest {
                largest = sum.abs();
                max = row;
            }
        }
        if lu[max][col].abs() < LU_SINGULARITY_THRESHOLD {
            return Ok(0.0); // singular, per LU convention
        }
        if max != col {
            lu.swap(max, col);
            even = !even;
        }
        let pivot = lu[col][col];
        for row in (col + 1)..m {
            lu[row][col] /= pivot;
        }
    }
    let mut det = if even { 1.0 } else { -1.0 };
    for (i, row) in lu.iter().enumerate() {
        det *= row[i];
    }
    Ok(det)
}

// ---------------------------------------------------------------------------
// QR (Householder, Commons Math conventions)
// ---------------------------------------------------------------------------

/// The internal Householder state: `qrt` (transposed working array) and the
/// diagonal of R. Mirrors Commons Math `QRDecomposition`'s fields.
struct QrState {
    qrt: Mat,
    r_diag: Vec<f64>,
    m: usize,
    n: usize,
}

fn qr_decompose(a: &Mat) -> Result<QrState> {
    let (m, n) = check_rect(a, "QR")?;
    // Commons Math stores the TRANSPOSE of A and reflects in place.
    let mut qrt: Mat = (0..n).map(|j| (0..m).map(|i| a[i][j]).collect()).collect();
    let mut r_diag = vec![0.0; m.min(n)];
    for minor in 0..m.min(n) {
        let norm_sq: f64 = (minor..m).map(|i| qrt[minor][i] * qrt[minor][i]).sum();
        let norm = norm_sq.sqrt();
        // Sign choice: a = -sign(pivot) * norm, exactly as Commons Math.
        let alpha = if qrt[minor][minor] > 0.0 { -norm } else { norm };
        r_diag[minor] = alpha;
        if alpha != 0.0 {
            qrt[minor][minor] -= alpha;
            for col in (minor + 1)..n {
                let mut dot = 0.0;
                for i in minor..m {
                    dot -= qrt[col][i] * qrt[minor][i];
                }
                let factor = dot / (alpha * qrt[minor][minor]);
                for i in minor..m {
                    let v = qrt[minor][i];
                    qrt[col][i] -= factor * v;
                }
            }
        }
    }
    Ok(QrState { qrt, r_diag, m, n })
}

/// Q factor (m×m, orthogonal) of the QR decomposition. Port of
/// `LinearAlgebra.qrQ` (Commons Math `QRDecomposition.getQ`).
pub fn qr_q(a: &Mat) -> Result<Mat> {
    let s = qr_decompose(a)?;
    let m = s.m;
    let p = s.m.min(s.n);
    // Build Q^T by applying the reflectors from the last minor to the first.
    let mut qt: Mat = vec![vec![0.0; m]; m];
    for (i, row) in qt.iter_mut().enumerate().skip(p) {
        row[i] = 1.0;
    }
    for minor in (0..p).rev() {
        qt[minor][minor] = 1.0;
        if s.qrt[minor][minor] != 0.0 {
            for col in minor..m {
                let mut alpha = 0.0;
                for row in minor..m {
                    alpha -= qt[col][row] * s.qrt[minor][row];
                }
                let alpha = alpha / (s.r_diag[minor] * s.qrt[minor][minor]);
                for row in minor..m {
                    qt[col][row] += -alpha * s.qrt[minor][row];
                }
            }
        }
    }
    // Q = (Q^T)^T
    Ok((0..m).map(|i| (0..m).map(|j| qt[j][i]).collect()).collect())
}

/// R factor (m×n, upper-triangular) of the QR decomposition. Port of
/// `LinearAlgebra.qrR` (Commons Math `QRDecomposition.getR`).
pub fn qr_r(a: &Mat) -> Result<Mat> {
    let s = qr_decompose(a)?;
    let mut r = vec![vec![0.0; s.n]; s.m];
    for row in 0..s.m.min(s.n) {
        r[row][row] = s.r_diag[row];
        for col in (row + 1)..s.n {
            r[row][col] = s.qrt[col][row];
        }
    }
    Ok(r)
}

// ---------------------------------------------------------------------------
// Cholesky
// ---------------------------------------------------------------------------

/// Relative symmetry threshold the Java call site passes to Commons Math.
const CHOLESKY_REL_SYMMETRY: f64 = 1.0e-9;
/// Absolute positivity threshold the Java call site passes to Commons Math.
const CHOLESKY_ABS_POSITIVITY: f64 = 1.0e-12;

/// Lower-triangular Cholesky factor L with `A = L·Lᵀ`. Port of
/// `LinearAlgebra.choleskyL`, including the relaxed thresholds chosen so
/// matrices assembled from solved (slightly noisy) variables still factor.
pub fn cholesky_l(a: &Mat) -> Result<Mat> {
    let n = check_square(a, "Cholesky")?;
    // lt starts as a copy; the loop turns it into the transposed factor Lᵀ,
    // mirroring the Commons Math in-place algorithm.
    let mut lt: Mat = a.clone();
    for i in 0..n {
        for j in (i + 1)..n {
            let lij = lt[i][j];
            let lji = lt[j][i];
            let max_delta = lij.abs().max(lji.abs()) * CHOLESKY_REL_SYMMETRY;
            if (lij - lji).abs() > max_delta {
                return Err(err(
                    "Cholesky requires a symmetric matrix (within tolerance).",
                ));
            }
            lt[j][i] = 0.0;
        }
    }
    for i in 0..n {
        if lt[i][i] <= CHOLESKY_ABS_POSITIVITY {
            return Err(err("Cholesky requires a positive-definite matrix."));
        }
        lt[i][i] = lt[i][i].sqrt();
        let inverse = 1.0 / lt[i][i];
        for q in ((i + 1)..n).rev() {
            lt[i][q] *= inverse;
            for p in q..n {
                let delta = lt[i][q] * lt[i][p];
                lt[q][p] -= delta;
            }
        }
    }
    // L = (Lᵀ)ᵀ
    Ok((0..n).map(|r| (0..n).map(|c| lt[c][r]).collect()).collect())
}

// ---------------------------------------------------------------------------
// Matrix exponential (scaling-and-squaring, [6/6] Padé)
// ---------------------------------------------------------------------------

/// Matrix exponential `e^A`. Line-for-line port of `LinearAlgebra.expm`
/// (scaling-and-squaring with a [6/6] Padé approximant).
pub fn expm(a: &Mat) -> Result<Mat> {
    let n = a.len();
    if n == 0 || a.iter().any(|row| row.len() != n) {
        return Err(err("MatExp requires a square matrix."));
    }
    // Scale A by 2^-s so its norm is < 1 (norm = max absolute column sum,
    // which is what Commons Math getNorm() computes). `libm` for wasm/native
    // bit determinism, per the port convention.
    let norm = (0..n)
        .map(|j| (0..n).map(|i| a[i][j].abs()).sum::<f64>())
        .fold(0.0f64, f64::max);
    let s = 0.max(libm::ceil(libm::log(norm.max(1.0e-300)) / libm::log(2.0)) as i64 + 1);
    let scale = 1.0 / 2.0f64.powi(s as i32);
    let scaled = scal_mat(a, scale);

    let ident = identity(n);
    let mut xk = scaled.clone();
    let mut c = 0.5;
    let mut num = add(&ident, &scal_mat(&scaled, c)); // numerator N
    let mut den = sub(&ident, &scal_mat(&scaled, c)); // denominator D
    let q = 6;
    let mut plus = true; // sign alternates in the denominator series
    for k in 2..=q {
        c = c * ((q - k + 1) as f64) / ((k * (2 * q - k + 1)) as f64);
        xk = mat_mul(&scaled, &xk);
        let cxk = scal_mat(&xk, c);
        num = add(&num, &cxk);
        den = if plus {
            add(&den, &cxk)
        } else {
            sub(&den, &cxk)
        };
        plus = !plus;
    }
    // F = D^{-1} N
    let mut f = lu_solve(&den, &num)?;
    // Undo the scaling: e^A = F^(2^s).
    for _ in 0..s {
        f = mat_mul(&f, &f);
    }
    Ok(f)
}

// ---------------------------------------------------------------------------
// SVD (one-sided Jacobi)
// ---------------------------------------------------------------------------

/// The three factors of a thin SVD, `A = U·S·Vᵀ`, with the Commons Math
/// shapes: for an m×n input and p = min(m, n), `u` is m×p, `s` is the p
/// singular values (non-increasing) and `v` is n×p.
pub struct Svd {
    pub u: Mat,
    pub s: Vec<f64>,
    pub v: Mat,
}

/// Singular values in non-increasing order (length `min(m, n)`). Port of
/// `LinearAlgebra.singularValues`.
pub fn singular_values(a: &Mat) -> Result<Vec<f64>> {
    Ok(svd(a)?.s)
}

/// Full thin SVD via one-sided Jacobi. See the module docs for the sign
/// convention (deterministic here, not guaranteed bit-identical to Commons
/// Math on sign-indeterminate columns).
pub fn svd(a: &Mat) -> Result<Svd> {
    let (m, n) = check_rect(a, "SVD")?;
    if m >= n {
        svd_tall(a, m, n)
    } else {
        // svd(Aᵀ) = (V, S, U): compute on the transpose and swap the factors.
        let t: Mat = (0..n).map(|i| (0..m).map(|j| a[j][i]).collect()).collect();
        let Svd { u, s, v } = svd_tall(&t, n, m)?;
        Ok(Svd { u: v, s, v: u })
    }
}

/// One-sided Jacobi on an m×n matrix with m >= n.
fn svd_tall(a: &Mat, m: usize, n: usize) -> Result<Svd> {
    let mut u: Mat = a.clone();
    let mut v = identity(n);
    let eps = 1.0e-12;
    let max_sweeps = 60;
    for _ in 0..max_sweeps {
        let mut off = 0.0f64;
        for p in 0..n {
            for q in (p + 1)..n {
                let mut alpha = 0.0;
                let mut beta = 0.0;
                let mut gamma = 0.0;
                for row in u.iter().take(m) {
                    alpha += row[p] * row[p];
                    beta += row[q] * row[q];
                    gamma += row[p] * row[q];
                }
                if gamma == 0.0 {
                    continue;
                }
                off = off.max(gamma.abs() / (alpha * beta).sqrt().max(f64::MIN_POSITIVE));
                if gamma.abs() <= eps * (alpha * beta).sqrt() {
                    continue;
                }
                // Jacobi rotation zeroing the (p, q) inner product.
                let zeta = (beta - alpha) / (2.0 * gamma);
                let t = zeta.signum() / (zeta.abs() + (1.0 + zeta * zeta).sqrt());
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = c * t;
                for row in u.iter_mut().take(m) {
                    let up = row[p];
                    let uq = row[q];
                    row[p] = c * up - s * uq;
                    row[q] = s * up + c * uq;
                }
                for row in v.iter_mut().take(n) {
                    let vp = row[p];
                    let vq = row[q];
                    row[p] = c * vp - s * vq;
                    row[q] = s * vp + c * vq;
                }
            }
        }
        if off <= eps {
            break;
        }
    }
    // Column norms are the singular values.
    let mut order: Vec<usize> = (0..n).collect();
    let norms: Vec<f64> = (0..n)
        .map(|j| (0..m).map(|i| u[i][j] * u[i][j]).sum::<f64>().sqrt())
        .collect();
    order.sort_by(|&x, &y| {
        norms[y]
            .partial_cmp(&norms[x])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut su = vec![vec![0.0; n]; m];
    let mut sv = vec![vec![0.0; n]; n];
    let mut sigma = vec![0.0; n];
    for (dst, &src) in order.iter().enumerate() {
        sigma[dst] = norms[src];
        // Deterministic sign: largest-|.| component of the V column positive.
        let mut max_idx = 0;
        for i in 1..n {
            if v[i][src].abs() > v[max_idx][src].abs() {
                max_idx = i;
            }
        }
        let flip = if v[max_idx][src] < 0.0 { -1.0 } else { 1.0 };
        for i in 0..m {
            su[i][dst] = if norms[src] > 0.0 {
                flip * u[i][src] / norms[src]
            } else {
                0.0
            };
        }
        for i in 0..n {
            sv[i][dst] = flip * v[i][src];
        }
    }
    Ok(Svd {
        u: su,
        s: sigma,
        v: sv,
    })
}

/// The p×p diagonal matrix of singular values (Commons Math `getS` shape as
/// used by the Java `flattenSvd`: p = min(m, n)).
pub fn svd_s_matrix(a: &Mat) -> Result<Mat> {
    let s = singular_values(a)?;
    let p = s.len();
    let mut out = vec![vec![0.0; p]; p];
    for (k, sk) in s.iter().enumerate() {
        out[k][k] = *sk;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// The synthetic `$`-intrinsic dispatcher
// ---------------------------------------------------------------------------

/// Evaluate a synthetic linear-algebra call the matrix expansion emits, from
/// its already-evaluated arguments (the flattened row-major matrix entries).
///
/// Handles exactly the names the Java `Evaluator` routes into
/// `LinearAlgebra`:
///
/// * `det$<n>`                 — LU determinant
/// * `qr$q$<i>$<j>$<m>$<n>`    — Q element; `qr$r$…` the R element
/// * `chol$l$<i>$<j>$<n>`      — Cholesky L element
/// * `expm$<i>$<j>$<n>`        — matrix-exponential element
/// * `svd$s$<k>$<m>$<n>`       — k-th singular value
/// * `svd$u|smat|v$<i>$<j>$<m>$<n>` — SVD factor elements
///
/// Returns `None` when `name` is not one of these (e.g. `eigen$…`, which has
/// no kernel here), so the evaluator can fall through to its own handling.
pub fn eval_intrinsic(name: &str, args: &[f64]) -> Option<Result<f64>> {
    let parts: Vec<&str> = name.split('$').collect();
    match parts.as_slice() {
        ["det", n] => {
            let n = parse_dim(n)?;
            Some(from_row_major(args, n, n).and_then(|m| det_lu(&m)))
        }
        ["qr", which @ ("q" | "r"), i, j, m, n] => {
            let (i, j, m, n) = (parse_dim(i)?, parse_dim(j)?, parse_dim(m)?, parse_dim(n)?);
            let factor = |a: &Mat| if *which == "q" { qr_q(a) } else { qr_r(a) };
            Some(from_row_major(args, m, n).and_then(|a| Ok(factor(&a)?[i][j])))
        }
        ["chol", "l", i, j, n] => {
            let (i, j, n) = (parse_dim(i)?, parse_dim(j)?, parse_dim(n)?);
            Some(from_row_major(args, n, n).and_then(|a| Ok(cholesky_l(&a)?[i][j])))
        }
        ["expm", i, j, n] => {
            let (i, j, n) = (parse_dim(i)?, parse_dim(j)?, parse_dim(n)?);
            Some(from_row_major(args, n, n).and_then(|a| Ok(expm(&a)?[i][j])))
        }
        ["svd", "s", k, m, n] => {
            let (k, m, n) = (parse_dim(k)?, parse_dim(m)?, parse_dim(n)?);
            Some(from_row_major(args, m, n).and_then(|a| Ok(singular_values(&a)?[k])))
        }
        ["svd", which @ ("u" | "smat" | "v"), i, j, m, n] => {
            let (i, j, m, n) = (parse_dim(i)?, parse_dim(j)?, parse_dim(m)?, parse_dim(n)?);
            Some(from_row_major(args, m, n).and_then(|a| match *which {
                "u" => Ok(svd(&a)?.u[i][j]),
                "smat" => Ok(svd_s_matrix(&a)?[i][j]),
                _ => Ok(svd(&a)?.v[i][j]),
            }))
        }
        _ => None,
    }
}

fn parse_dim(text: &str) -> Option<usize> {
    text.parse::<usize>().ok()
}

// ---------------------------------------------------------------------------
// Small dense helpers
// ---------------------------------------------------------------------------

fn check_square(a: &Mat, what: &str) -> Result<usize> {
    let n = a.len();
    if n == 0 || a.iter().any(|row| row.len() != n) {
        return Err(err(format!("{what} requires a non-empty square matrix")));
    }
    Ok(n)
}

fn check_rect(a: &Mat, what: &str) -> Result<(usize, usize)> {
    let m = a.len();
    if m == 0 || a[0].is_empty() {
        return Err(err(format!("{what} requires a non-empty matrix")));
    }
    let n = a[0].len();
    if a.iter().any(|row| row.len() != n) {
        return Err(err(format!("{what} requires a rectangular matrix")));
    }
    Ok((m, n))
}

fn identity(n: usize) -> Mat {
    let mut m = vec![vec![0.0; n]; n];
    for (i, row) in m.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    m
}

fn scal_mat(a: &Mat, factor: f64) -> Mat {
    a.iter()
        .map(|row| row.iter().map(|v| v * factor).collect())
        .collect()
}

fn add(a: &Mat, b: &Mat) -> Mat {
    a.iter()
        .zip(b)
        .map(|(ra, rb)| ra.iter().zip(rb).map(|(x, y)| x + y).collect())
        .collect()
}

fn sub(a: &Mat, b: &Mat) -> Mat {
    a.iter()
        .zip(b)
        .map(|(ra, rb)| ra.iter().zip(rb).map(|(x, y)| x - y).collect())
        .collect()
}

fn mat_mul(a: &Mat, b: &Mat) -> Mat {
    let rows = a.len();
    let inner = b.len();
    let cols = b[0].len();
    let mut out = vec![vec![0.0; cols]; rows];
    for i in 0..rows {
        for k in 0..inner {
            let aik = a[i][k];
            for j in 0..cols {
                out[i][j] += aik * b[k][j];
            }
        }
    }
    out
}

/// Solve `A·X = B` for X via LU with partial pivoting (used by [`expm`]'s
/// Padé step, mirroring the Commons Math `LUDecomposition` solver).
fn lu_solve(a: &Mat, b: &Mat) -> Result<Mat> {
    let n = check_square(a, "linear solve")?;
    let cols = b[0].len();
    let mut lu = a.clone();
    let mut perm: Vec<usize> = (0..n).collect();
    for col in 0..n {
        for row in 0..col {
            let mut sum = lu[row][col];
            for i in 0..row {
                sum -= lu[row][i] * lu[i][col];
            }
            lu[row][col] = sum;
        }
        let mut max = col;
        let mut largest = f64::NEG_INFINITY;
        for row in col..n {
            let mut sum = lu[row][col];
            for i in 0..col {
                sum -= lu[row][i] * lu[i][col];
            }
            lu[row][col] = sum;
            if sum.abs() > largest {
                largest = sum.abs();
                max = row;
            }
        }
        if lu[max][col].abs() < LU_SINGULARITY_THRESHOLD {
            return Err(err("linear solve: matrix is singular"));
        }
        if max != col {
            lu.swap(max, col);
            perm.swap(max, col);
        }
        let pivot = lu[col][col];
        for row in (col + 1)..n {
            lu[row][col] /= pivot;
        }
    }
    // Apply the permutation to B, then forward/back substitution.
    let mut y: Mat = perm.iter().map(|&p| b[p].clone()).collect();
    for col in 0..n {
        for row in (col + 1)..n {
            for j in 0..cols {
                let delta = lu[row][col] * y[col][j];
                y[row][j] -= delta;
            }
        }
    }
    for col in (0..n).rev() {
        for j in 0..cols {
            y[col][j] /= lu[col][col];
        }
        for row in 0..col {
            for j in 0..cols {
                let delta = lu[row][col] * y[col][j];
                y[row][j] -= delta;
            }
        }
    }
    Ok(y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_mat_eq(actual: &Mat, expected: &[&[f64]], tol: f64) {
        assert_eq!(actual.len(), expected.len(), "row count");
        for (i, row) in actual.iter().enumerate() {
            assert_eq!(row.len(), expected[i].len(), "col count in row {i}");
            for (j, v) in row.iter().enumerate() {
                assert!(
                    (v - expected[i][j]).abs() < tol,
                    "[{i}][{j}]: {v} vs {}",
                    expected[i][j]
                );
            }
        }
    }

    #[test]
    fn det_lu_matches_known_determinants() {
        let a = vec![vec![4.0, 0.0], vec![0.0, 5.0]];
        assert!((det_lu(&a).unwrap() - 20.0).abs() < 1e-12);
        // 4x4 with det = 2 (checked against numpy.linalg.det).
        let b = vec![
            vec![1.0, 2.0, 3.0, 4.0],
            vec![5.0, 6.0, 7.0, 8.5],
            vec![9.0, 10.0, 12.0, 12.0],
            vec![13.0, 14.0, 15.0, 17.0],
        ];
        let expected = 2.0;
        assert!(
            (det_lu(&b).unwrap() - expected).abs() < 1e-9,
            "got {}",
            det_lu(&b).unwrap()
        );
    }

    #[test]
    fn det_lu_of_singular_matrix_is_zero() {
        let a = vec![vec![1.0, 2.0], vec![2.0, 4.0]];
        assert_eq!(det_lu(&a).unwrap(), 0.0);
    }

    #[test]
    fn qr_reconstructs_and_is_orthogonal() {
        let a = vec![
            vec![12.0, -51.0, 4.0],
            vec![6.0, 167.0, -68.0],
            vec![-4.0, 24.0, -41.0],
        ];
        let q = qr_q(&a).unwrap();
        let r = qr_r(&a).unwrap();
        // R upper-triangular.
        for i in 0..3 {
            for j in 0..i {
                assert!(r[i][j].abs() < 1e-10, "R[{i}][{j}] = {}", r[i][j]);
            }
        }
        // Q orthogonal: QᵀQ = I.
        let qt: Mat = (0..3).map(|i| (0..3).map(|j| q[j][i]).collect()).collect();
        let qtq = mat_mul(&qt, &q);
        assert_mat_eq(
            &qtq,
            &[&[1.0, 0.0, 0.0], &[0.0, 1.0, 0.0], &[0.0, 0.0, 1.0]],
            1e-10,
        );
        // Q·R = A.
        let qr = mat_mul(&q, &r);
        assert_mat_eq(
            &qr,
            &[
                &[12.0, -51.0, 4.0],
                &[6.0, 167.0, -68.0],
                &[-4.0, 24.0, -41.0],
            ],
            1e-9,
        );
        // Commons Math sign choice: R[0][0] = -norm(first column) for a
        // positive pivot.
        assert!((r[0][0] + 14.0).abs() < 1e-10, "R[0][0] = {}", r[0][0]);
    }

    #[test]
    fn qr_handles_rectangular_input() {
        let a = vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![5.0, 6.0]];
        let q = qr_q(&a).unwrap(); // 3x3
        let r = qr_r(&a).unwrap(); // 3x2
        assert_eq!((q.len(), q[0].len()), (3, 3));
        assert_eq!((r.len(), r[0].len()), (3, 2));
        let qr = mat_mul(&q, &r);
        assert_mat_eq(&qr, &[&[1.0, 2.0], &[3.0, 4.0], &[5.0, 6.0]], 1e-9);
    }

    #[test]
    fn cholesky_factors_a_spd_matrix() {
        let a = vec![
            vec![4.0, 12.0, -16.0],
            vec![12.0, 37.0, -43.0],
            vec![-16.0, -43.0, 98.0],
        ];
        let l = cholesky_l(&a).unwrap();
        assert_mat_eq(
            &l,
            &[&[2.0, 0.0, 0.0], &[6.0, 1.0, 0.0], &[-8.0, 5.0, 3.0]],
            1e-10,
        );
    }

    #[test]
    fn cholesky_rejects_non_spd() {
        let asym = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        assert!(cholesky_l(&asym).is_err());
        let indefinite = vec![vec![-1.0, 0.0], vec![0.0, 1.0]];
        assert!(cholesky_l(&indefinite).is_err());
    }

    #[test]
    fn expm_of_zero_and_diagonal() {
        let z = vec![vec![0.0, 0.0], vec![0.0, 0.0]];
        assert_mat_eq(&expm(&z).unwrap(), &[&[1.0, 0.0], &[0.0, 1.0]], 1e-12);
        let d = vec![vec![1.0, 0.0], vec![0.0, 2.0]];
        assert_mat_eq(
            &expm(&d).unwrap(),
            &[&[1.0f64.exp(), 0.0], &[0.0, 2.0f64.exp()]],
            1e-9,
        );
    }

    #[test]
    fn expm_of_rotation_generator() {
        // exp([[0, -t], [t, 0]]) = [[cos t, -sin t], [sin t, cos t]].
        let t = 1.2;
        let a = vec![vec![0.0, -t], vec![t, 0.0]];
        let e = expm(&a).unwrap();
        assert_mat_eq(&e, &[&[t.cos(), -t.sin()], &[t.sin(), t.cos()]], 1e-10);
    }

    #[test]
    fn singular_values_match_known_case() {
        // A = [[3, 0], [0, -2]] has singular values 3, 2.
        let a = vec![vec![3.0, 0.0], vec![0.0, -2.0]];
        let s = singular_values(&a).unwrap();
        assert!((s[0] - 3.0).abs() < 1e-10);
        assert!((s[1] - 2.0).abs() < 1e-10);
    }

    #[test]
    fn svd_reconstructs_the_input() {
        let a = vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![5.0, 6.0]];
        let f = svd(&a).unwrap();
        assert_eq!((f.u.len(), f.u[0].len()), (3, 2));
        assert_eq!(f.s.len(), 2);
        assert_eq!((f.v.len(), f.v[0].len()), (2, 2));
        assert!(f.s[0] >= f.s[1]);
        // U·S·Vᵀ = A
        let mut us = vec![vec![0.0; 2]; 3];
        for i in 0..3 {
            for j in 0..2 {
                us[i][j] = f.u[i][j] * f.s[j];
            }
        }
        let vt: Mat = (0..2)
            .map(|i| (0..2).map(|j| f.v[j][i]).collect())
            .collect();
        let back = mat_mul(&us, &vt);
        assert_mat_eq(&back, &[&[1.0, 2.0], &[3.0, 4.0], &[5.0, 6.0]], 1e-9);
    }

    #[test]
    fn svd_of_wide_matrix_swaps_factors() {
        let a = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]];
        let f = svd(&a).unwrap();
        assert_eq!((f.u.len(), f.u[0].len()), (2, 2));
        assert_eq!(f.s.len(), 2);
        assert_eq!((f.v.len(), f.v[0].len()), (3, 2));
    }

    #[test]
    fn eval_intrinsic_dispatches_det() {
        let v = eval_intrinsic("det$2", &[4.0, 0.0, 0.0, 5.0])
            .unwrap()
            .unwrap();
        assert!((v - 20.0).abs() < 1e-12);
    }

    #[test]
    fn eval_intrinsic_dispatches_qr_and_chol_and_expm_and_svd() {
        // chol$l$0$0$2 of [[4, 0], [0, 9]] = 2.
        let v = eval_intrinsic("chol$l$0$0$2", &[4.0, 0.0, 0.0, 9.0])
            .unwrap()
            .unwrap();
        assert!((v - 2.0).abs() < 1e-12);
        // expm$0$0$1 of [0] = 1.
        let v = eval_intrinsic("expm$0$0$1", &[0.0]).unwrap().unwrap();
        assert!((v - 1.0).abs() < 1e-12);
        // svd$s$0$2$2 of diag(3, 2) = 3.
        let v = eval_intrinsic("svd$s$0$2$2", &[3.0, 0.0, 0.0, 2.0])
            .unwrap()
            .unwrap();
        assert!((v - 3.0).abs() < 1e-10);
        // qr$r$0$0$2$2 of I = -1 (Householder sign choice).
        let v = eval_intrinsic("qr$r$0$0$2$2", &[1.0, 0.0, 0.0, 1.0])
            .unwrap()
            .unwrap();
        assert!((v + 1.0).abs() < 1e-12);
    }

    #[test]
    fn eval_intrinsic_ignores_foreign_names() {
        assert!(eval_intrinsic("eigen$val$0$2", &[1.0, 0.0, 0.0, 1.0]).is_none());
        assert!(eval_intrinsic("prop$enthalpy$water$t$p", &[1.0]).is_none());
        assert!(eval_intrinsic("sqrt", &[4.0]).is_none());
        // Malformed dims fall through rather than panicking.
        assert!(eval_intrinsic("det$x", &[1.0]).is_none());
    }

    #[test]
    fn eval_intrinsic_rejects_wrong_entry_count() {
        assert!(eval_intrinsic("det$2", &[1.0, 2.0, 3.0]).unwrap().is_err());
    }
}
