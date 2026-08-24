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
//! * [`singular_values`] / [`svd`] — a line-faithful transcription of Commons
//!   Math 3.6.1's `SingularValueDecomposition` (JAMA-derived Golub–Kahan
//!   bidiagonalisation + implicit-shift QR), **including the column signs of
//!   U and V**, which are whatever the Householder reflector sign choices
//!   produce. The previous one-sided-Jacobi kernel with an invented
//!   largest-component-positive sign rule was ledger item 24's divergence;
//!   the balreal/SVD goldens compare signs element-exact, so do not "clean
//!   up" the signs with any normalisation pass.

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
// SVD (Commons Math / JAMA Golub–Kahan)
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

/// Full thin SVD: a line-faithful transcription of Commons Math 3.6.1's
/// `SingularValueDecomposition` constructor (JAMA-derived) — Golub–Kahan
/// bidiagonalisation by Householder reflections, then the implicit-shift QR
/// sweep, with a wide input transposed up front and the factors swapped back
/// ("m" is always the largest dimension), exactly as the Java does.
///
/// Faithfulness deliberately includes the **column signs** of U and V, which
/// are whatever the reflector sign choices produce. The balreal/SVD parity
/// goldens compare U/V elements sign-exact (ledger item 24 was the previous
/// Jacobi kernel's invented sign rule), so no normalisation may be applied
/// on top. The kase-1/2/3/4 structure, thresholds (`TINY + EPS·…`) and the
/// NaN-tolerant `!(|e[k]| > threshold)` comparison (MATH-947) are the Java's.
#[allow(clippy::too_many_lines)]
pub fn svd(a: &Mat) -> Result<Svd> {
    let (rows, cols) = check_rect(a, "SVD")?;
    // Commons Math: EPS = 0x1.0p-52 (= f64::EPSILON), TINY = 0x1.0p-966.
    let eps = f64::EPSILON;
    let tiny = 2.0f64.powi(-966);
    let transposed = rows < cols;
    let (m, n) = if transposed {
        (cols, rows)
    } else {
        (rows, cols)
    };
    let mut aw: Mat = if transposed {
        (0..cols)
            .map(|i| (0..rows).map(|j| a[j][i]).collect())
            .collect()
    } else {
        a.clone()
    };

    let mut sv = vec![0.0f64; n];
    let mut u = vec![vec![0.0f64; n]; m];
    let mut v = vec![vec![0.0f64; n]; n];
    let mut e = vec![0.0f64; n];
    let mut work = vec![0.0f64; m];

    // Reduce A to bidiagonal form, storing the diagonal elements in sv and
    // the super-diagonal elements in e.
    let nct = (m - 1).min(n);
    let nrt = n.saturating_sub(2);
    for k in 0..nct.max(nrt) {
        if k < nct {
            // Compute the transformation for the k-th column and place the
            // k-th diagonal in sv[k] (2-norm without under/overflow).
            sv[k] = 0.0;
            for i in k..m {
                sv[k] = sv[k].hypot(aw[i][k]);
            }
            if sv[k] != 0.0 {
                if aw[k][k] < 0.0 {
                    sv[k] = -sv[k];
                }
                for i in k..m {
                    aw[i][k] /= sv[k];
                }
                aw[k][k] += 1.0;
            }
            sv[k] = -sv[k];
        }
        for j in (k + 1)..n {
            if k < nct && sv[k] != 0.0 {
                // Apply the transformation.
                let mut t = 0.0;
                for i in k..m {
                    t += aw[i][k] * aw[i][j];
                }
                t = -t / aw[k][k];
                for i in k..m {
                    aw[i][j] += t * aw[i][k];
                }
            }
            // Place the k-th row of A into e for the subsequent row
            // transformation.
            e[j] = aw[k][j];
        }
        if k < nct {
            // Place the transformation in U for subsequent back
            // multiplication.
            for i in k..m {
                u[i][k] = aw[i][k];
            }
        }
        if k < nrt {
            // Compute the k-th row transformation and place the k-th
            // super-diagonal in e[k].
            e[k] = 0.0;
            for i in (k + 1)..n {
                e[k] = e[k].hypot(e[i]);
            }
            if e[k] != 0.0 {
                if e[k + 1] < 0.0 {
                    e[k] = -e[k];
                }
                for i in (k + 1)..n {
                    e[i] /= e[k];
                }
                e[k + 1] += 1.0;
            }
            e[k] = -e[k];
            if k + 1 < m && e[k] != 0.0 {
                // Apply the transformation.
                for item in work.iter_mut().skip(k + 1) {
                    *item = 0.0;
                }
                for j in (k + 1)..n {
                    for i in (k + 1)..m {
                        work[i] += e[j] * aw[i][j];
                    }
                }
                for j in (k + 1)..n {
                    let t = -e[j] / e[k + 1];
                    for i in (k + 1)..m {
                        aw[i][j] += t * work[i];
                    }
                }
            }
            // Place the transformation in V for subsequent back
            // multiplication.
            for i in (k + 1)..n {
                v[i][k] = e[i];
            }
        }
    }

    // Set up the final bidiagonal matrix of order p.
    let mut p = n;
    if nct < n {
        sv[nct] = aw[nct][nct];
    }
    if m < p {
        sv[p - 1] = 0.0;
    }
    if nrt + 1 < p {
        e[nrt] = aw[nrt][p - 1];
    }
    e[p - 1] = 0.0;

    // Generate U.
    for j in nct..n {
        for i in 0..m {
            u[i][j] = 0.0;
        }
        u[j][j] = 1.0;
    }
    for k in (0..nct).rev() {
        if sv[k] != 0.0 {
            for j in (k + 1)..n {
                let mut t = 0.0;
                for i in k..m {
                    t += u[i][k] * u[i][j];
                }
                t = -t / u[k][k];
                for i in k..m {
                    u[i][j] += t * u[i][k];
                }
            }
            for i in k..m {
                u[i][k] = -u[i][k];
            }
            u[k][k] += 1.0;
            // Java: for (i = 0; i < k - 1; i++) — empty when k = 0.
            for i in 0..k.saturating_sub(1) {
                u[i][k] = 0.0;
            }
        } else {
            for i in 0..m {
                u[i][k] = 0.0;
            }
            u[k][k] = 1.0;
        }
    }

    // Generate V.
    for k in (0..n).rev() {
        if k < nrt && e[k] != 0.0 {
            for j in (k + 1)..n {
                let mut t = 0.0;
                for i in (k + 1)..n {
                    t += v[i][k] * v[i][j];
                }
                t = -t / v[k + 1][k];
                for i in (k + 1)..n {
                    v[i][j] += t * v[i][k];
                }
            }
        }
        for i in 0..n {
            v[i][k] = 0.0;
        }
        v[k][k] = 1.0;
    }

    // Main iteration loop for the singular values.
    let pp = p - 1;
    while p > 0 {
        // kase = 1  if sv[p-1] and e[k-1] are negligible and k < p
        // kase = 2  if sv[k] is negligible and k < p
        // kase = 3  if e[k-1] is negligible, k < p, and sv[k..p] are not
        //           (one QR step)
        // kase = 4  if e[p-2] is negligible (convergence)
        let mut k: isize = p as isize - 2;
        while k >= 0 {
            let ku = k as usize;
            let threshold = tiny + eps * (sv[ku].abs() + sv[ku + 1].abs());
            // Written `!(… > threshold)` so a NaN takes the break instead of
            // looping forever (Commons Math issue MATH-947).
            if !(e[ku].abs() > threshold) {
                e[ku] = 0.0;
                break;
            }
            k -= 1;
        }
        let kase;
        if k == p as isize - 2 {
            kase = 4;
        } else {
            let mut ks: isize = p as isize - 1;
            while ks >= k {
                if ks == k {
                    break;
                }
                let ksu = ks as usize;
                let t = if ks != p as isize { e[ksu].abs() } else { 0.0 }
                    + if ks != k + 1 { e[ksu - 1].abs() } else { 0.0 };
                if sv[ksu].abs() <= tiny + eps * t {
                    sv[ksu] = 0.0;
                    break;
                }
                ks -= 1;
            }
            if ks == k {
                kase = 3;
            } else if ks == p as isize - 1 {
                kase = 1;
            } else {
                kase = 2;
                k = ks;
            }
        }
        let k = (k + 1) as usize;
        match kase {
            // Deflate negligible sv[p-1].
            1 => {
                let mut f = e[p - 2];
                e[p - 2] = 0.0;
                for j in (k..=(p - 2)).rev() {
                    let mut t = sv[j].hypot(f);
                    let cs = sv[j] / t;
                    let sn = f / t;
                    sv[j] = t;
                    if j != k {
                        f = -sn * e[j - 1];
                        e[j - 1] *= cs;
                    }
                    for i in 0..n {
                        t = cs * v[i][j] + sn * v[i][p - 1];
                        v[i][p - 1] = -sn * v[i][j] + cs * v[i][p - 1];
                        v[i][j] = t;
                    }
                }
            }
            // Split at negligible sv[k].
            2 => {
                let mut f = e[k - 1];
                e[k - 1] = 0.0;
                for j in k..p {
                    let mut t = sv[j].hypot(f);
                    let cs = sv[j] / t;
                    let sn = f / t;
                    sv[j] = t;
                    f = -sn * e[j];
                    e[j] *= cs;
                    for i in 0..m {
                        t = cs * u[i][j] + sn * u[i][k - 1];
                        u[i][k - 1] = -sn * u[i][j] + cs * u[i][k - 1];
                        u[i][j] = t;
                    }
                }
            }
            // One QR step.
            3 => {
                // Calculate the shift.
                let max_pm1_pm2 = sv[p - 1].abs().max(sv[p - 2].abs());
                let scale = max_pm1_pm2
                    .max(e[p - 2].abs())
                    .max(sv[k].abs())
                    .max(e[k].abs());
                let sp = sv[p - 1] / scale;
                let spm1 = sv[p - 2] / scale;
                let epm1 = e[p - 2] / scale;
                let sk = sv[k] / scale;
                let ek = e[k] / scale;
                let b = ((spm1 + sp) * (spm1 - sp) + epm1 * epm1) / 2.0;
                let c = (sp * epm1) * (sp * epm1);
                let mut shift = 0.0;
                if b != 0.0 || c != 0.0 {
                    shift = (b * b + c).sqrt();
                    if b < 0.0 {
                        shift = -shift;
                    }
                    shift = c / (b + shift);
                }
                let mut f = (sk + sp) * (sk - sp) + shift;
                let mut g = sk * ek;
                // Chase zeros.
                for j in k..(p - 1) {
                    let mut t = f.hypot(g);
                    let mut cs = f / t;
                    let mut sn = g / t;
                    if j != k {
                        e[j - 1] = t;
                    }
                    f = cs * sv[j] + sn * e[j];
                    e[j] = cs * e[j] - sn * sv[j];
                    g = sn * sv[j + 1];
                    sv[j + 1] *= cs;
                    for i in 0..n {
                        t = cs * v[i][j] + sn * v[i][j + 1];
                        v[i][j + 1] = -sn * v[i][j] + cs * v[i][j + 1];
                        v[i][j] = t;
                    }
                    t = f.hypot(g);
                    cs = f / t;
                    sn = g / t;
                    sv[j] = t;
                    f = cs * e[j] + sn * sv[j + 1];
                    sv[j + 1] = -sn * e[j] + cs * sv[j + 1];
                    g = sn * e[j + 1];
                    e[j + 1] *= cs;
                    if j < m - 1 {
                        for i in 0..m {
                            t = cs * u[i][j] + sn * u[i][j + 1];
                            u[i][j + 1] = -sn * u[i][j] + cs * u[i][j + 1];
                            u[i][j] = t;
                        }
                    }
                }
                e[p - 2] = f;
            }
            // Convergence.
            _ => {
                // Make the singular values positive.
                if sv[k] <= 0.0 {
                    sv[k] = if sv[k] < 0.0 { -sv[k] } else { 0.0 };
                    for i in 0..=pp {
                        v[i][k] = -v[i][k];
                    }
                }
                // Order the singular values.
                let mut k = k;
                while k < pp {
                    if sv[k] >= sv[k + 1] {
                        break;
                    }
                    sv.swap(k, k + 1);
                    if k < n - 1 {
                        for i in 0..n {
                            v[i].swap(k, k + 1);
                        }
                    }
                    if k < m - 1 {
                        for i in 0..m {
                            u[i].swap(k, k + 1);
                        }
                    }
                    k += 1;
                }
                p -= 1;
            }
        }
    }

    // A wide input was decomposed as its transpose: swap the factors back.
    Ok(if transposed {
        Svd { u: v, s: sv, v: u }
    } else {
        Svd { u, s: sv, v }
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
// Inverse, general solve, pseudo-inverse
// ---------------------------------------------------------------------------

/// Solve `A·X = B` by LU with partial pivoting — the Commons Math
/// `LUDecomposition.getSolver().solve(B)` semantics, including the `1e-11`
/// singularity threshold (a singular `A` is an error, not a pseudo-solve).
///
/// Added in Phase 9: the control-design suite (`crate::control::design`)
/// needs a general dense solve, which the Phase-4 kernels only had privately.
pub fn solve(a: &Mat, b: &Mat) -> Result<Mat> {
    lu_solve(a, b)
}

/// Matrix inverse via LU — Commons Math `MatrixUtils.inverse(m)`, which is
/// `new LUDecomposition(m).getSolver().getInverse()`.
pub fn inverse(a: &Mat) -> Result<Mat> {
    let n = check_square(a, "inverse")?;
    lu_solve(a, &identity(n))
}

/// Moore–Penrose pseudo-inverse via the SVD — Commons Math
/// `SingularValueDecomposition.getSolver().getInverse()`, including its
/// cut-off `tol = max(m, n) · s₀ · eps` below which a singular value is
/// treated as zero.
///
/// The Java `ControllerDesign.lyap`/`dlyap`/`dare` fall back to this solver
/// whenever the LU decomposition reports the Kronecker system singular.
pub fn pinv(a: &Mat) -> Result<Mat> {
    let (m, n) = check_rect(a, "pseudo-inverse")?;
    let f = svd(a)?;
    let tol = (m.max(n) as f64) * f.s[0] * 2.220446049250313e-16;
    // pinv = V · diag(1/sᵢ) · Uᵀ, dropping the directions at or below `tol`.
    let p = f.s.len();
    let mut out = vec![vec![0.0; m]; n];
    for i in 0..n {
        for j in 0..m {
            let mut sum = 0.0;
            for k in 0..p {
                if f.s[k] > tol {
                    sum += f.v[i][k] * f.u[j][k] / f.s[k];
                }
            }
            out[i][j] = sum;
        }
    }
    Ok(out)
}

/// Commons Math's `SingularValueDecomposition.getSolver()`, which is a
/// **pseudo-inverse**: `x = V · diag(1/σᵢ for σᵢ > tol) · Uᵀ · b`, never a
/// singularity error. Singular values at or below the threshold are dropped, so
/// a rank-deficient `A` yields the minimum-norm least-squares answer instead of
/// failing the solve.
///
/// Kept as a struct rather than a free function because both callers decompose
/// once and solve for several right-hand sides: `analysis::uncertainty`'s
/// propagation (one `b` per uncertainty source) and — since ledger item 40 —
/// `solver::newton`'s singular-Jacobian fallback.
pub struct SvdSolver {
    svd: Svd,
    tol: f64,
}

impl SvdSolver {
    /// Decompose `a` and fix the singular-value cut-off.
    pub fn new(a: &Mat) -> Result<SvdSolver> {
        let svd = svd(a)?;
        let rows = a.len();
        let cols = a.first().map_or(0, Vec::len);
        // Commons Math `SingularValueDecomposition`:
        //     tol = max(m * singularValues[0] * EPS, sqrt(Precision.SAFE_MIN))
        // where `m` is the LARGER dimension (the constructor transposes so that
        // "m is always the largest dimension"), `EPS` is `0x1.0p-52`
        // (`f64::EPSILON`) and `SAFE_MIN` is `0x1.0p-1022`
        // (`f64::MIN_POSITIVE`).
        let m = rows.max(cols) as f64;
        let s0 = svd.s.first().copied().unwrap_or(0.0);
        let tol = (m * s0 * f64::EPSILON).max(f64::MIN_POSITIVE.sqrt());
        Ok(SvdSolver { svd, tol })
    }

    /// `pinv(A) · b`, with `b` indexed by row of the decomposed matrix.
    pub fn solve(&self, b: &[f64]) -> Vec<f64> {
        let n = self.svd.v.len();
        let mut x = vec![0.0; n];
        for (k, &s) in self.svd.s.iter().enumerate() {
            if !(s > self.tol) {
                continue;
            }
            let mut dot = 0.0;
            for (i, bi) in b.iter().enumerate() {
                dot += self.svd.u[i][k] * bi;
            }
            let coefficient = dot / s;
            for (i, xi) in x.iter_mut().enumerate() {
                *xi += coefficient * self.svd.v[i][k];
            }
        }
        x
    }
}

/// Solve `A·X = B` with LU, falling back to the SVD pseudo-inverse when `A`
/// is singular. Mirrors the Java pattern
/// `LUDecomposition(...).getSolver()`, `if (!isNonSingular()) SingularValueDecomposition(...)`.
pub fn solve_or_pinv(a: &Mat, b: &Mat) -> Result<Mat> {
    match lu_solve(a, b) {
        Ok(x) => Ok(x),
        Err(_) => Ok(mat_mul(&pinv(a)?, b)),
    }
}

// ---------------------------------------------------------------------------
// Eigen-decomposition of a general real matrix
// ---------------------------------------------------------------------------

/// Eigenvalues and eigenvectors of a **general** (not necessarily symmetric)
/// real matrix.
///
/// The layout is the EISPACK / Commons Math one: a complex conjugate pair
/// occupies two consecutive slots `j`, `j+1` with `im[j] > 0`, `im[j+1] < 0`,
/// and columns `j`, `j+1` of [`Eigen::v`] hold the **real** and **imaginary**
/// parts of the common eigenvector. Real eigenvalues carry `im = 0` and an
/// ordinary real column.
#[derive(Debug, Clone, PartialEq)]
pub struct Eigen {
    /// Real parts of the eigenvalues, in real-Schur diagonal order.
    pub re: Vec<f64>,
    /// Imaginary parts; `0.0` for a real eigenvalue.
    pub im: Vec<f64>,
    /// Eigenvector matrix (n×n), columns aligned with `re`/`im`. Not
    /// normalised — Commons Math does not normalise this path either.
    pub v: Mat,
}

/// Commons Math `SchurTransformer.MAX_ITERATIONS`: the QR sweep gives up
/// rather than spinning. JAMA's `hqr2`, which this transcribes, has no cap at
/// all; in a browser an unbounded loop is not a debugging inconvenience but a
/// hung tab, so the Commons Math bound is the one that ships.
const SCHUR_MAX_ITERATIONS: usize = 100;

/// Eigen-decomposition of a general real matrix: Householder reduction to
/// upper Hessenberg form (`orthes`) followed by the Francis double-shift QR
/// iteration with eigenvector back-substitution (`hqr2`).
///
/// This is the algorithm behind Commons Math's `EigenDecomposition` for a
/// non-symmetric input (`HessenbergTransformer` + `SchurTransformer` +
/// `findEigenVectors`), so eigenvalue **order** — which the Java's
/// `PolynomialHelpers.roots` and `ControllerDesign.dare` inherit — matches in
/// the cases that matter.
///
/// # Divergence
///
/// Commons Math tests the input for symmetry first and, when it is symmetric,
/// takes a tridiagonal QL path that additionally **sorts the eigenvalues in
/// decreasing order** and returns orthonormal vectors. That branch is not
/// reproduced *here*: a symmetric input goes through the general path, so the
/// values agree but the vector scaling need not.
/// [`crate::control::tf::eigenvalues`] re-applies the decreasing sort on top.
///
/// # Non-finite input
///
/// A matrix containing a NaN or an infinity is **refused**. The Java reaches
/// the same observable outcome by a longer road — Commons Math's QR sweep
/// simply never converges and throws
/// `MaxCountExceededException: illegal state: convergence failed`, which
/// `PolynomialHelpers.roots` rewraps — but the failure has to be *detected*
/// rather than hoped for: every deflation test on a NaN compares false, so the
/// iteration can also terminate early and hand back a finite-looking answer
/// that is entirely fictitious. Measured on `rlocus(num, den, 2)`, whose
/// `(i−1)/(M−2)` gain schedule divides `0/0`: the Java throws, and an
/// unguarded transcription of this kernel returns poles at `−1, −1`.
pub fn eigen(a: &Mat) -> Result<Eigen> {
    let n = check_square(a, "eigen")?;
    if a.iter().flatten().any(|v| !v.is_finite()) {
        return Err(err(
            "eigen: the matrix contains a non-finite entry, so no eigenvalue \
             can be computed",
        ));
    }
    let mut h = a.clone();
    let mut v = identity(n);
    orthes(&mut h, &mut v, n);
    let (re, im) = hqr2(&mut h, &mut v, n)?;
    Ok(Eigen { re, im, v })
}

/// Householder reduction of `h` to upper Hessenberg form, accumulating the
/// orthogonal transform into `v`. Port of the EISPACK `orthes`/`ortran` pair.
fn orthes(h: &mut Mat, v: &mut Mat, n: usize) {
    let high = n.saturating_sub(1);
    let mut ort = vec![0.0; n];
    for m in 1..high {
        // Scale the column below the diagonal.
        let mut scale = 0.0;
        for i in m..=high {
            scale += h[i][m - 1].abs();
        }
        if scale == 0.0 {
            continue;
        }
        // The Householder vector, built from the bottom up as EISPACK does.
        let mut hh = 0.0;
        for i in (m..=high).rev() {
            ort[i] = h[i][m - 1] / scale;
            hh += ort[i] * ort[i];
        }
        let mut g = hh.sqrt();
        if ort[m] > 0.0 {
            g = -g;
        }
        hh -= ort[m] * g;
        ort[m] -= g;

        // H := (I - u u'/h) H (I - u u'/h)
        for j in m..n {
            let mut f = 0.0;
            for i in (m..=high).rev() {
                f += ort[i] * h[i][j];
            }
            f /= hh;
            for i in m..=high {
                h[i][j] -= f * ort[i];
            }
        }
        for i in 0..=high {
            let mut f = 0.0;
            for j in (m..=high).rev() {
                f += ort[j] * h[i][j];
            }
            f /= hh;
            for j in m..=high {
                h[i][j] -= f * ort[j];
            }
        }
        ort[m] *= scale;
        h[m][m - 1] = scale * g;
    }

    // Accumulate the reflectors into V (`ortran`), last minor first.
    for m in (1..high).rev() {
        if h[m][m - 1] == 0.0 {
            continue;
        }
        for i in (m + 1)..=high {
            ort[i] = h[i][m - 1];
        }
        for j in m..=high {
            let mut g = 0.0;
            for i in m..=high {
                g += ort[i] * v[i][j];
            }
            // The double division is EISPACK's underflow guard; keep it.
            g = (g / ort[m]) / h[m][m - 1];
            for i in m..=high {
                v[i][j] += g * ort[i];
            }
        }
    }
}

/// Complex division `(xr + i·xi) / (yr + i·yi)`, EISPACK's `cdiv` — scaled by
/// the larger denominator part so the intermediate does not overflow.
fn cdiv(xr: f64, xi: f64, yr: f64, yi: f64) -> (f64, f64) {
    if yr.abs() > yi.abs() {
        let r = yi / yr;
        let d = yr + r * yi;
        ((xr + r * xi) / d, (xi - r * xr) / d)
    } else {
        let r = yr / yi;
        let d = yi + r * yr;
        ((r * xr + xi) / d, (r * xi - xr) / d)
    }
}

/// Francis double-shift QR on the Hessenberg matrix `h`, accumulating into
/// `v`, then back-substitution for the eigenvectors. Port of EISPACK `hqr2`
/// (the algorithm Commons Math's `SchurTransformer` +
/// `EigenDecomposition.findEigenVectors` implement).
#[allow(clippy::too_many_lines)]
fn hqr2(h: &mut Mat, v: &mut Mat, nn: usize) -> Result<(Vec<f64>, Vec<f64>)> {
    let mut d = vec![0.0; nn];
    let mut e = vec![0.0; nn];
    let eps = f64::EPSILON / 2.0; // 2^-53 * 2 = 2^-52, EISPACK's `eps`
    let mut exshift = 0.0;
    let (mut p, mut q, mut r, mut s, mut z) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut t, mut w, mut x, mut y);

    // Matrix norm over the Hessenberg band.
    let mut norm = 0.0;
    for i in 0..nn {
        for j in i.saturating_sub(1)..nn {
            norm += h[i][j].abs();
        }
    }

    let mut iter = 0usize;
    let mut n = nn as isize - 1;
    while n >= 0 {
        let nu = n as usize;
        // Look for a single small sub-diagonal element.
        let mut l = n;
        while l > 0 {
            let lu = l as usize;
            s = h[lu - 1][lu - 1].abs() + h[lu][lu].abs();
            if s == 0.0 {
                s = norm;
            }
            if h[lu][lu - 1].abs() < eps * s {
                break;
            }
            l -= 1;
        }

        if l == n {
            // One root found.
            h[nu][nu] += exshift;
            d[nu] = h[nu][nu];
            e[nu] = 0.0;
            n -= 1;
            iter = 0;
        } else if l == n - 1 {
            // Two roots found.
            w = h[nu][nu - 1] * h[nu - 1][nu];
            p = (h[nu - 1][nu - 1] - h[nu][nu]) / 2.0;
            q = p * p + w;
            z = q.abs().sqrt();
            h[nu][nu] += exshift;
            h[nu - 1][nu - 1] += exshift;
            x = h[nu][nu];

            if q >= 0.0 {
                // Real pair.
                z = if p >= 0.0 { p + z } else { p - z };
                d[nu - 1] = x + z;
                d[nu] = d[nu - 1];
                if z != 0.0 {
                    d[nu] = x - w / z;
                }
                e[nu - 1] = 0.0;
                e[nu] = 0.0;
                x = h[nu][nu - 1];
                s = x.abs() + z.abs();
                p = x / s;
                q = z / s;
                r = (p * p + q * q).sqrt();
                p /= r;
                q /= r;

                for j in (nu - 1)..nn {
                    z = h[nu - 1][j];
                    h[nu - 1][j] = q * z + p * h[nu][j];
                    h[nu][j] = q * h[nu][j] - p * z;
                }
                for i in 0..=nu {
                    z = h[i][nu - 1];
                    h[i][nu - 1] = q * z + p * h[i][nu];
                    h[i][nu] = q * h[i][nu] - p * z;
                }
                for i in 0..nn {
                    z = v[i][nu - 1];
                    v[i][nu - 1] = q * z + p * v[i][nu];
                    v[i][nu] = q * v[i][nu] - p * z;
                }
            } else {
                // Complex pair.
                d[nu - 1] = x + p;
                d[nu] = x + p;
                e[nu - 1] = z;
                e[nu] = -z;
            }
            n -= 2;
            iter = 0;
        } else {
            // No convergence yet — form the shift.
            x = h[nu][nu];
            y = 0.0;
            w = 0.0;
            if l < n {
                y = h[nu - 1][nu - 1];
                w = h[nu][nu - 1] * h[nu - 1][nu];
            }

            // Wilkinson's original ad-hoc shift.
            if iter == 10 {
                exshift += x;
                for i in 0..=nu {
                    h[i][i] -= x;
                }
                s = h[nu][nu - 1].abs() + h[nu - 1][nu - 2].abs();
                x = 0.75 * s;
                y = x;
                w = -0.4375 * s * s;
            }
            // MATLAB's later ad-hoc shift.
            if iter == 30 {
                s = (y - x) / 2.0;
                s = s * s + w;
                if s > 0.0 {
                    s = s.sqrt();
                    if y < x {
                        s = -s;
                    }
                    s = x - w / ((y - x) / 2.0 + s);
                    for i in 0..=nu {
                        h[i][i] -= s;
                    }
                    exshift += s;
                    x = 0.964;
                    y = 0.964;
                    w = 0.964;
                }
            }
            iter += 1;
            if iter > SCHUR_MAX_ITERATIONS {
                return Err(err(
                    "eigen: the QR iteration did not converge (matrix too ill-conditioned)",
                ));
            }

            // Look for two consecutive small sub-diagonal elements.
            let mut m = n - 2;
            while m >= l {
                let mu = m as usize;
                z = h[mu][mu];
                r = x - z;
                s = y - z;
                p = (r * s - w) / h[mu + 1][mu] + h[mu][mu + 1];
                q = h[mu + 1][mu + 1] - z - r - s;
                r = h[mu + 2][mu + 1];
                s = p.abs() + q.abs() + r.abs();
                p /= s;
                q /= s;
                r /= s;
                if m == l {
                    break;
                }
                if h[mu][mu - 1].abs() * (q.abs() + r.abs())
                    < eps
                        * (p.abs() * (h[mu - 1][mu - 1].abs() + z.abs() + h[mu + 1][mu + 1].abs()))
                {
                    break;
                }
                m -= 1;
            }
            let mu = m as usize;
            for i in (mu + 2)..=nu {
                h[i][i - 2] = 0.0;
                if i > mu + 2 {
                    h[i][i - 3] = 0.0;
                }
            }

            // The double QR step over rows l..n and columns m..n.
            for k in mu..nu {
                let notlast = k != nu - 1;
                if k != mu {
                    p = h[k][k - 1];
                    q = h[k + 1][k - 1];
                    r = if notlast { h[k + 2][k - 1] } else { 0.0 };
                    x = p.abs() + q.abs() + r.abs();
                    if x == 0.0 {
                        continue;
                    }
                    p /= x;
                    q /= x;
                    r /= x;
                }
                s = (p * p + q * q + r * r).sqrt();
                if p < 0.0 {
                    s = -s;
                }
                if s == 0.0 {
                    continue;
                }
                if k != mu {
                    h[k][k - 1] = -s * x;
                } else if l != m {
                    h[k][k - 1] = -h[k][k - 1];
                }
                p += s;
                x = p / s;
                y = q / s;
                z = r / s;
                q /= p;
                r /= p;

                for j in k..nn {
                    p = h[k][j] + q * h[k + 1][j];
                    if notlast {
                        p += r * h[k + 2][j];
                        h[k + 2][j] -= p * z;
                    }
                    h[k][j] -= p * x;
                    h[k + 1][j] -= p * y;
                }
                for i in 0..=nu.min(k + 3) {
                    p = x * h[i][k] + y * h[i][k + 1];
                    if notlast {
                        p += z * h[i][k + 2];
                        h[i][k + 2] -= p * r;
                    }
                    h[i][k] -= p;
                    h[i][k + 1] -= p * q;
                }
                for i in 0..nn {
                    p = x * v[i][k] + y * v[i][k + 1];
                    if notlast {
                        p += z * v[i][k + 2];
                        v[i][k + 2] -= p * r;
                    }
                    v[i][k] -= p;
                    v[i][k + 1] -= p * q;
                }
            }
        }
    }

    // Back-substitute for the vectors of the quasi-triangular form.
    if norm == 0.0 {
        return Ok((d, e));
    }
    for nb in (0..nn).rev() {
        p = d[nb];
        q = e[nb];
        if q == 0.0 {
            // Real vector.
            let mut l = nb;
            h[nb][nb] = 1.0;
            for i in (0..nb).rev() {
                w = h[i][i] - p;
                r = 0.0;
                for j in l..=nb {
                    r += h[i][j] * h[j][nb];
                }
                if e[i] < 0.0 {
                    z = w;
                    s = r;
                    continue;
                }
                l = i;
                if e[i] == 0.0 {
                    h[i][nb] = if w != 0.0 { -r / w } else { -r / (eps * norm) };
                } else {
                    // Solve the 2×2 real system.
                    x = h[i][i + 1];
                    y = h[i + 1][i];
                    q = (d[i] - p) * (d[i] - p) + e[i] * e[i];
                    t = (x * s - z * r) / q;
                    h[i][nb] = t;
                    h[i + 1][nb] = if x.abs() > z.abs() {
                        (-r - w * t) / x
                    } else {
                        (-s - y * t) / z
                    };
                }
                // Overflow control.
                t = h[i][nb].abs();
                if (eps * t) * t > 1.0 {
                    for j in i..=nb {
                        h[j][nb] /= t;
                    }
                }
            }
        } else if q < 0.0 {
            // Complex vector; `nb` is the second (negative-imaginary) slot.
            let mut l = nb - 1;
            if h[nb][nb - 1].abs() > h[nb - 1][nb].abs() {
                h[nb - 1][nb - 1] = q / h[nb][nb - 1];
                h[nb - 1][nb] = -(h[nb][nb] - p) / h[nb][nb - 1];
            } else {
                let (cr, ci) = cdiv(0.0, -h[nb - 1][nb], h[nb - 1][nb - 1] - p, q);
                h[nb - 1][nb - 1] = cr;
                h[nb - 1][nb] = ci;
            }
            h[nb][nb - 1] = 0.0;
            h[nb][nb] = 1.0;
            for i in (0..nb.saturating_sub(1)).rev() {
                let mut ra = 0.0;
                let mut sa = 0.0;
                for j in l..=nb {
                    ra += h[i][j] * h[j][nb - 1];
                    sa += h[i][j] * h[j][nb];
                }
                w = h[i][i] - p;
                if e[i] < 0.0 {
                    z = w;
                    r = ra;
                    s = sa;
                    continue;
                }
                l = i;
                if e[i] == 0.0 {
                    let (cr, ci) = cdiv(-ra, -sa, w, q);
                    h[i][nb - 1] = cr;
                    h[i][nb] = ci;
                } else {
                    // Solve the 2×2 complex system.
                    x = h[i][i + 1];
                    y = h[i + 1][i];
                    let mut vr = (d[i] - p) * (d[i] - p) + e[i] * e[i] - q * q;
                    let vi = (d[i] - p) * 2.0 * q;
                    if vr == 0.0 && vi == 0.0 {
                        vr = eps * norm * (w.abs() + q.abs() + x.abs() + y.abs() + z.abs());
                    }
                    let (cr, ci) = cdiv(x * r - z * ra + q * sa, x * s - z * sa - q * ra, vr, vi);
                    h[i][nb - 1] = cr;
                    h[i][nb] = ci;
                    if x.abs() > z.abs() + q.abs() {
                        h[i + 1][nb - 1] = (-ra - w * h[i][nb - 1] + q * h[i][nb]) / x;
                        h[i + 1][nb] = (-sa - w * h[i][nb] - q * h[i][nb - 1]) / x;
                    } else {
                        let (cr2, ci2) = cdiv(-r - y * h[i][nb - 1], -s - y * h[i][nb], z, q);
                        h[i + 1][nb - 1] = cr2;
                        h[i + 1][nb] = ci2;
                    }
                }
                // Overflow control.
                t = h[i][nb - 1].abs().max(h[i][nb].abs());
                if (eps * t) * t > 1.0 {
                    for j in i..=nb {
                        h[j][nb - 1] /= t;
                        h[j][nb] /= t;
                    }
                }
            }
        }
    }

    // Back-transform to the eigenvectors of the original matrix.
    for j in (0..nn).rev() {
        for i in 0..nn {
            let mut acc = 0.0;
            for k in 0..=j {
                acc += v[i][k] * h[k][j];
            }
            v[i][j] = acc;
        }
    }
    Ok((d, e))
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
/// * `eigen$val|re|im$<k>$<n>` — k-th eigenvalue (ascending), or its
///   real/imaginary part in the complex-spectrum form
/// * `eigen$vec$<i>$<k>$<n>`   — component `i` of the k-th eigenvector
///   (unit 2-norm, largest-|·| component made positive)
///
/// Returns `None` when `name` is not one of these, so the evaluator can fall
/// through to its own handling.
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
        ["eigen", kind @ ("val" | "re" | "im"), k, n] => {
            let (k, n) = (parse_dim(k)?, parse_dim(n)?);
            Some(from_row_major(args, n, n).and_then(|a| {
                let e = eigen(&a)?;
                // val is a real-spectrum form; re/im deliberately are not.
                if *kind == "val" {
                    check_real_spectrum(&e)?;
                }
                let idx = eigen_order(&e)[k];
                Ok(if *kind == "im" { e.im[idx] } else { e.re[idx] })
            }))
        }
        ["eigen", "vec", i, k, n] => {
            let (i, k, n) = (parse_dim(i)?, parse_dim(k)?, parse_dim(n)?);
            Some(from_row_major(args, n, n).and_then(|a| {
                let e = eigen(&a)?;
                check_real_spectrum(&e)?;
                let col = eigen_order(&e)[k];
                // Port of the tail of `Evaluator.evalEigen`: unit 2-norm, then
                // the largest-magnitude component made positive. The magnitude
                // scan is strictly-greater, so an exact tie keeps the LOWEST
                // index — the `[[2,1],[1,2]]` golden's column 1 is (+,−)/√2
                // only because of that tie-break; `>=` would flip it.
                let mut v: Vec<f64> = (0..n).map(|r| e.v[r][col]).collect();
                let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
                for x in &mut v {
                    *x /= norm;
                }
                let mut max_idx = 0;
                for r in 1..n {
                    if v[r].abs() > v[max_idx].abs() {
                        max_idx = r;
                    }
                }
                if v[max_idx] < 0.0 {
                    for x in &mut v {
                        *x = -*x;
                    }
                }
                Ok(v[i])
            }))
        }
        _ => None,
    }
}

/// The frEES kernel's eigenpair ordering (`Evaluator.evalEigen`): **ascending**
/// by real part, then by imaginary part — Java's
/// `comparingDouble(..).thenComparingDouble(..)`, whose `Double.compare`
/// semantics `f64::total_cmp` reproduces (−0.0 before +0.0, NaN last). This is
/// frEES's own sort applied *on top of* whatever the decomposition returned;
/// it is not [`crate::control::tf::eigenvalues`]'s decreasing symmetric-path
/// sort, and the two must not be shared.
fn eigen_order(e: &Eigen) -> Vec<usize> {
    let mut order: Vec<usize> = (0..e.re.len()).collect();
    order.sort_by(|&a, &b| {
        e.re[a]
            .total_cmp(&e.re[b])
            .then(e.im[a].total_cmp(&e.im[b]))
    });
    order
}

/// Port of the `hasComplexEigenvalues()` refusal in `Evaluator.evalEigen`.
/// Commons Math's test is `!Precision.equals(im, 0.0, 1e-12)`, i.e. complex
/// iff any |im| exceeds 1e-12. The message is the Java's, verbatim.
fn check_real_spectrum(e: &Eigen) -> Result<()> {
    if e.im.iter().any(|im| im.abs() > 1e-12) {
        return Err(err(
            "Matrix has complex eigenvalues; this form supports real spectra \
             only (symmetric matrices always qualify). Use the two-output form \
             CALL Eigenvalues(A : re, im) to get a complex spectrum as \
             real/imaginary parts.",
        ));
    }
    Ok(())
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

/// The n×n identity.
pub fn identity(n: usize) -> Mat {
    let mut m = vec![vec![0.0; n]; n];
    for (i, row) in m.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    m
}

/// `factor · A`.
pub fn scal_mat(a: &Mat, factor: f64) -> Mat {
    a.iter()
        .map(|row| row.iter().map(|v| v * factor).collect())
        .collect()
}

/// `A + B` (same shape).
pub fn add(a: &Mat, b: &Mat) -> Mat {
    a.iter()
        .zip(b)
        .map(|(ra, rb)| ra.iter().zip(rb).map(|(x, y)| x + y).collect())
        .collect()
}

/// `A − B` (same shape).
pub fn sub(a: &Mat, b: &Mat) -> Mat {
    a.iter()
        .zip(b)
        .map(|(ra, rb)| ra.iter().zip(rb).map(|(x, y)| x - y).collect())
        .collect()
}

/// `Aᵀ`.
pub fn transpose(a: &Mat) -> Mat {
    if a.is_empty() || a[0].is_empty() {
        return Vec::new();
    }
    let (m, n) = (a.len(), a[0].len());
    (0..n).map(|j| (0..m).map(|i| a[i][j]).collect()).collect()
}

/// `A · B`.
///
/// A degenerate operand (no rows, or rows of width zero) yields a matrix with
/// the corresponding dimension zero rather than a panic — the state-space
/// interconnections in `crate::control::design` legitimately build blocks for
/// a subsystem with no states, and an index panic there is a wasm abort.
pub fn mat_mul(a: &Mat, b: &Mat) -> Mat {
    let rows = a.len();
    let inner = b.len();
    let cols = if inner == 0 { 0 } else { b[0].len() };
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
        // `eigen$val$…` left this list when ledger item 34 wired the eigen
        // kernels — see the eigen_* dispatch tests below.
        assert!(eval_intrinsic("prop$enthalpy$water$t$p", &[1.0]).is_none());
        assert!(eval_intrinsic("sqrt", &[4.0]).is_none());
        // Malformed dims fall through rather than panicking.
        assert!(eval_intrinsic("det$x", &[1.0]).is_none());
        assert!(eval_intrinsic("eigen$val$x$2", &[1.0; 4]).is_none());
    }

    /// The `eigen$…` synthetics (ledger item 34): ordering is the frEES
    /// kernel's ascending (real, imag) sort, NOT Commons Math's raw order and
    /// NOT `control::tf::eigenvalues`'s decreasing symmetric sort.
    #[test]
    fn eigen_val_intrinsic_sorts_ascending() {
        // [[2,1],[1,2]] has eigenvalues {1, 3}; ascending puts 1 first. This is
        // the `eqsys-solves-eigenvalues-of-symmetric-matrix` golden's shape.
        let a = [2.0, 1.0, 1.0, 2.0];
        let l0 = eval_intrinsic("eigen$val$0$2", &a).unwrap().unwrap();
        let l1 = eval_intrinsic("eigen$val$1$2", &a).unwrap().unwrap();
        assert!((l0 - 1.0).abs() < 1e-9, "{l0}");
        assert!((l1 - 3.0).abs() < 1e-9, "{l1}");
        // A diagonal written in descending order still reads back ascending.
        let d = [3.0, 0.0, 0.0, 2.0];
        let d0 = eval_intrinsic("eigen$val$0$2", &d).unwrap().unwrap();
        let d1 = eval_intrinsic("eigen$val$1$2", &d).unwrap().unwrap();
        assert!((d0 - 2.0).abs() < 1e-12, "{d0}");
        assert!((d1 - 3.0).abs() < 1e-12, "{d1}");
    }

    /// The vec form's sign convention, pinned against the
    /// `eqsys-solves-eigen-decomposition-with-vectors-and-downstream-equations`
    /// golden: column 1 (λ=1) is (+,−)/√2 — the strictly-greater magnitude
    /// tie-break keeps the LOWEST index on an exact tie, so the first
    /// component is made positive. A `>=` scan would flip the column.
    #[test]
    fn eigen_vec_intrinsic_matches_the_java_sign_convention() {
        let a = [2.0, 1.0, 1.0, 2.0];
        let s = std::f64::consts::FRAC_1_SQRT_2;
        let expect = [[s, s], [-s, s]]; // V[i][k], columns are eigenvectors
        for i in 0..2 {
            for k in 0..2 {
                let got = eval_intrinsic(&format!("eigen$vec${i}${k}$2"), &a)
                    .unwrap()
                    .unwrap();
                assert!(
                    (got - expect[i][k]).abs() < 1e-9,
                    "V[{i},{k}] = {got}, want {}",
                    expect[i][k]
                );
            }
        }
    }

    /// val/vec refuse a complex spectrum with the Java's message; re/im carry
    /// it as parts, ascending by (real, imag).
    #[test]
    fn eigen_intrinsics_route_complex_spectra_to_the_re_im_form() {
        // [[0,-1],[1,0]] rotates: eigenvalues ±i.
        let a = [0.0, -1.0, 1.0, 0.0];
        let err = eval_intrinsic("eigen$val$0$2", &a).unwrap().unwrap_err();
        assert!(err.to_string().contains("complex eigenvalues"), "{err}");
        let err = eval_intrinsic("eigen$vec$0$0$2", &a).unwrap().unwrap_err();
        assert!(err.to_string().contains("complex eigenvalues"), "{err}");
        let re0 = eval_intrinsic("eigen$re$0$2", &a).unwrap().unwrap();
        let im0 = eval_intrinsic("eigen$im$0$2", &a).unwrap().unwrap();
        let im1 = eval_intrinsic("eigen$im$1$2", &a).unwrap().unwrap();
        assert!(re0.abs() < 1e-12, "{re0}");
        assert!((im0 - -1.0).abs() < 1e-9, "{im0}"); // ascending: −i before +i
        assert!((im1 - 1.0).abs() < 1e-9, "{im1}");
    }

    #[test]
    fn eval_intrinsic_rejects_wrong_entry_count() {
        assert!(eval_intrinsic("det$2", &[1.0, 2.0, 3.0]).unwrap().is_err());
    }

    // -----------------------------------------------------------------------
    // Phase-9 additions: inverse / solve / pinv / eigen
    // -----------------------------------------------------------------------

    #[test]
    fn inverse_reproduces_the_identity_and_refuses_a_singular_matrix() {
        let a = vec![vec![4.0, 7.0], vec![2.0, 6.0]];
        let inv = inverse(&a).unwrap();
        assert_mat_eq(&inv, &[&[0.6, -0.7], &[-0.2, 0.4]], 1e-12);
        assert_mat_eq(&mat_mul(&a, &inv), &[&[1.0, 0.0], &[0.0, 1.0]], 1e-12);
        assert!(inverse(&vec![vec![1.0, 2.0], vec![2.0, 4.0]]).is_err());
    }

    #[test]
    fn solve_answers_a_known_system() {
        // [[2,1],[1,3]] x = [[5],[10]]  ->  x = [1, 3]
        let x = solve(
            &vec![vec![2.0, 1.0], vec![1.0, 3.0]],
            &vec![vec![5.0], vec![10.0]],
        )
        .unwrap();
        assert_mat_eq(&x, &[&[1.0], &[3.0]], 1e-12);
    }

    #[test]
    fn pinv_satisfies_the_moore_penrose_identity_on_a_rank_deficient_matrix() {
        let a = vec![vec![1.0, 2.0], vec![2.0, 4.0]];
        let p = pinv(&a).unwrap();
        // A A⁺ A = A is the defining property that survives rank deficiency.
        let back = mat_mul(&mat_mul(&a, &p), &a);
        assert_mat_eq(&back, &[&[1.0, 2.0], &[2.0, 4.0]], 1e-9);
    }

    #[test]
    fn solve_or_pinv_falls_back_when_the_lu_reports_singular() {
        let a = vec![vec![1.0, 2.0], vec![2.0, 4.0]];
        let b = vec![vec![1.0], vec![2.0]];
        // Consistent but singular: LU refuses, the pseudo-inverse answers.
        assert!(solve(&a, &b).is_err());
        let x = solve_or_pinv(&a, &b).unwrap();
        let residual = sub(&mat_mul(&a, &x), &b);
        for row in &residual {
            for v in row {
                assert!(v.abs() < 1e-9, "least-squares residual {v}");
            }
        }
    }

    #[test]
    fn eigen_finds_real_eigenpairs() {
        // Symmetric 2×2 with eigenvalues 1 and 3.
        let a = vec![vec![2.0, 1.0], vec![1.0, 2.0]];
        let e = eigen(&a).unwrap();
        let mut vals: Vec<f64> = e.re.clone();
        vals.sort_by(|x, y| x.partial_cmp(y).unwrap());
        assert!((vals[0] - 1.0).abs() < 1e-12, "{vals:?}");
        assert!((vals[1] - 3.0).abs() < 1e-12, "{vals:?}");
        assert!(e.im.iter().all(|v| *v == 0.0));
        // A v = λ v, column by column.
        for j in 0..2 {
            let v: Vec<f64> = (0..2).map(|i| e.v[i][j]).collect();
            for i in 0..2 {
                let av: f64 = (0..2).map(|k| a[i][k] * v[k]).sum();
                assert!((av - e.re[j] * v[i]).abs() < 1e-10, "A v != lambda v");
            }
        }
    }

    #[test]
    fn eigen_stores_a_complex_pair_as_real_and_imaginary_columns() {
        // Rotation generator: eigenvalues ±2i.
        let a = vec![vec![0.0, -2.0], vec![2.0, 0.0]];
        let e = eigen(&a).unwrap();
        assert!(e.re.iter().all(|v| v.abs() < 1e-12), "{:?}", e.re);
        assert!((e.im[0] - 2.0).abs() < 1e-12, "{:?}", e.im);
        assert!((e.im[1] + 2.0).abs() < 1e-12, "{:?}", e.im);
        // Columns 0 and 1 are Re(v) and Im(v) of the λ = 0 + 2i eigenvector:
        //   A·vr = λr·vr − λi·vi   and   A·vi = λi·vr + λr·vi.
        let (lr, li) = (e.re[0], e.im[0]);
        let vr: Vec<f64> = (0..2).map(|i| e.v[i][0]).collect();
        let vi: Vec<f64> = (0..2).map(|i| e.v[i][1]).collect();
        for i in 0..2 {
            let avr: f64 = (0..2).map(|k| a[i][k] * vr[k]).sum();
            let avi: f64 = (0..2).map(|k| a[i][k] * vi[k]).sum();
            assert!((avr - (lr * vr[i] - li * vi[i])).abs() < 1e-10, "Re part");
            assert!((avi - (li * vr[i] + lr * vi[i])).abs() < 1e-10, "Im part");
        }
    }

    /// The eigenvalue ORDER is user-visible through `PolynomialHelpers.roots`
    /// and the root locus. This companion matrix is `s⁴ + 7s³ + 14s² + 8s`,
    /// and the sequence below is what the Java oracle returns.
    #[test]
    fn eigen_reproduces_the_commons_math_ordering_of_a_companion_matrix() {
        let c = [1.0, 7.0, 14.0, 8.0, 0.0];
        let degree = 4;
        let mut a = vec![vec![0.0; degree]; degree];
        for j in 0..degree {
            a[0][j] = -c[j + 1] / c[0];
        }
        for i in 1..degree {
            a[i][i - 1] = 1.0;
        }
        let e = eigen(&a).unwrap();
        let expected = [-4.0, -2.0, -1.0, 0.0];
        for (i, want) in expected.iter().enumerate() {
            assert!(
                (e.re[i] - want).abs() < 1e-9,
                "eigenvalue {i}: {} vs {want}",
                e.re[i]
            );
            assert!(e.im[i].abs() < 1e-12);
        }
    }

    #[test]
    fn eigen_handles_a_defective_matrix_without_hanging() {
        // A single Jordan block: one eigenvalue 2 with algebraic multiplicity
        // 3 and a one-dimensional eigenspace. The QR iteration must terminate.
        let a = vec![
            vec![2.0, 1.0, 0.0],
            vec![0.0, 2.0, 1.0],
            vec![0.0, 0.0, 2.0],
        ];
        let e = eigen(&a).unwrap();
        for i in 0..3 {
            assert!((e.re[i] - 2.0).abs() < 1e-9, "{:?}", e.re);
            assert!(e.im[i].abs() < 1e-9);
        }
    }

    #[test]
    fn eigen_rejects_a_non_square_matrix() {
        assert!(eigen(&vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]]).is_err());
    }

    /// A NaN entry must be an error, not a plausible-looking answer. Without
    /// the guard this exact matrix — the companion form `rlocus(…, M = 2)`
    /// builds — comes back with eigenvalues `−1, −1`, while the Java throws
    /// `convergence failed`.
    #[test]
    fn eigen_refuses_a_non_finite_matrix_instead_of_inventing_eigenvalues() {
        let nan = vec![vec![-2.0, f64::NAN], vec![1.0, 0.0]];
        let e = eigen(&nan).unwrap_err();
        assert!(e.to_string().contains("non-finite"), "{e}");
        assert!(eigen(&vec![vec![f64::INFINITY, 0.0], vec![0.0, 1.0]]).is_err());
    }

    #[test]
    fn transpose_and_mat_mul_tolerate_degenerate_shapes() {
        assert!(transpose(&Vec::new()).is_empty());
        assert_eq!(transpose(&vec![vec![1.0, 2.0]]), vec![vec![1.0], vec![2.0]]);
        // A block for a subsystem with no states: shapes collapse, no panic.
        let empty: Mat = Vec::new();
        let zero_wide: Mat = vec![Vec::new()];
        let one_empty_row: Mat = vec![Vec::new()];
        assert_eq!(mat_mul(&vec![vec![1.0, 2.0]], &empty), one_empty_row);
        assert_eq!(mat_mul(&vec![vec![1.0]], &zero_wide), one_empty_row);
    }
}
