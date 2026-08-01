//! State-feedback controller design.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/cas/ControllerDesign.java`
//! (912 LOC): continuous- and discrete-time LQR, pole placement, Lyapunov and
//! Riccati solvers, estimator (LQE) gains, gramians, balanced realisation,
//! state-space interconnections, Padé delay approximants, step metrics and the
//! root locus.
//!
//! # The Riccati route is the matrix sign function, not an ordered Schur form
//!
//! The continuous ARE `A'P + PA − PBR⁻¹B'P + Q = 0` is solved here exactly as
//! the Java solves it: by the **matrix sign function** of the Hamiltonian
//! `H = [A, −BR⁻¹B'; −Q, −A']`, iterated as `Z ← ½(cZ + (cZ)⁻¹)` with
//! determinant scaling, then a least-squares solve of
//! `[S₁₂; S₂₂+I] P = −[S₁₁+I; S₂₁]`. That needs only real LU inverses. A
//! stable/anti-stable eigenvalue **ordering** never enters, so the classic
//! failure mode — mixing the two invariant subspaces and returning a gain that
//! looks plausible and does not stabilise — cannot arise on this path.
//!
//! The *discrete* [`dare`] is the one that does select an invariant subspace:
//! it takes the eigenvectors of the symplectic matrix whose eigenvalues lie
//! **inside the unit disc**. `X = V₂₁V₁₁⁻¹` is invariant under any change of
//! basis of that subspace, so neither the eigenvalue order nor the scaling of
//! [`crate::linalg::eigen`]'s vectors can perturb it — only mis-*selecting*
//! could, and the `|λ| < 1` test is exact.
//!
//! # Linear algebra
//!
//! The Java sits on Apache Commons Math. Everything used here is in
//! [`crate::linalg`]: `svd` (rank, balanced realisation), `det_lu`, `cholesky_l`
//! and the Phase-9 additions `inverse`, `solve`, `pinv`, `solve_or_pinv` and
//! `eigen` (a general real eigen-decomposition — genuinely missing before, and
//! needed by both [`dare`] and the polynomial root finder).

// Float equality here is transcribed, not accidental: `det != 0.0`,
// `if (z != 0.0)`, `sign == 0.0` and friends are exact-zero *structural* tests
// in the Java (and in the EISPACK-derived code it calls), where a tolerance
// would change which branch runs. The NaN-rejecting guards written `!(x > 0.0)`
// are negated for the same reason they are in `crate::linalg`.
#![allow(clippy::neg_cmp_op_on_partial_ord)]
// Numerical kernels index parallel arrays and `a[i][j]` slices by shared loop
// variables, mirroring the Java being transcribed. Iterator rewrites obscure
// the correspondence.
#![allow(clippy::needless_range_loop)]
// The state-space interconnections take two complete (A, B, C, D) quadruples.
// Grouping them into a struct is `control::ss`'s call to make, not this
// module's — the Java signature is what `ControlSystemsEvaluator` calls.
#![allow(clippy::too_many_arguments)]

use crate::control::ss::StateSpaceMatrices;
use crate::control::tf::{self, Complex};
use crate::diag::{FreesError, Result};
use crate::linalg::{self, Mat};

/// Java `ControllerDesign.SIGN_MAX_ITERS`.
const SIGN_MAX_ITERS: usize = 100;
/// Java `ControllerDesign.SIGN_TOL`.
const SIGN_TOL: f64 = 1e-12;
/// Commons Math `Precision.EPSILON`, transcribed as the Java writes it.
const MACHINE_EPS: f64 = 2.220446049250313e-16;

fn err(message: impl Into<String>) -> FreesError {
    FreesError::evaluation(message)
}

// ---------------------------------------------------------------------------
// Result shapes
// ---------------------------------------------------------------------------

/// Root-locus samples: the gains `k` and, per gain, the closed-loop pole real
/// and imaginary parts. Java `ControllerDesign.RlocusResult`.
#[derive(Debug, Clone, PartialEq)]
pub struct RlocusResult {
    /// The swept gains (`k[0]` is always 0).
    pub k: Vec<f64>,
    /// Closed-loop pole real parts, `k.len()` × `deg(den)`.
    pub cpr: Mat,
    /// Closed-loop pole imaginary parts, same shape as `cpr`.
    pub cpi: Mat,
}

/// The balanced triple returned by [`balreal`]. Java
/// `ControllerDesign.BalrealResult`.
#[derive(Debug, Clone, PartialEq)]
pub struct BalrealResult {
    /// Balanced state matrix `T⁻¹AT`.
    pub a: Mat,
    /// Balanced input matrix `T⁻¹B`.
    pub b: Mat,
    /// Balanced output matrix `CT`.
    pub c: Mat,
}

// ---------------------------------------------------------------------------
// Structural properties: rank, controllability, observability, similarity
// ---------------------------------------------------------------------------

/// Numerical rank via the SVD. Port of `ControllerDesign.rank`, including its
/// floor on the tolerance (`tol = max(m, n)·s₀·eps`, but never below `1e-14`).
pub fn rank(matrix: &Mat) -> Result<usize> {
    let (rows, cols) = shape(matrix, "rank")?;
    let s = linalg::singular_values(matrix)?;
    if s.is_empty() {
        return Ok(0);
    }
    let max_dim = rows.max(cols) as f64;
    let mut tol = max_dim * s[0] * MACHINE_EPS;
    if tol < 1e-14 {
        tol = 1e-14;
    }
    Ok(s.iter().filter(|v| **v > tol).count())
}

/// Controllability matrix `[B, AB, …, Aⁿ⁻¹B]` (n × n·m). Port of
/// `ControllerDesign.ctrb`.
pub fn ctrb(a: &Mat, b: &Mat) -> Result<Mat> {
    let n = square(a, "ctrb")?;
    let (br, m) = shape(b, "ctrb")?;
    if br != n {
        return Err(err(format!(
            "ctrb: B must have {n} rows to match A, got {br}"
        )));
    }
    let mut out = vec![vec![0.0; n * m]; n];
    let mut block = b.clone();
    for i in 0..n {
        for r in 0..n {
            for c in 0..m {
                out[r][i * m + c] = block[r][c];
            }
        }
        block = linalg::mat_mul(a, &block);
    }
    Ok(out)
}

/// Observability matrix `[C; CA; …; CAⁿ⁻¹]` (n·p × n). Port of
/// `ControllerDesign.obsv`.
pub fn obsv(a: &Mat, c: &Mat) -> Result<Mat> {
    let n = square(a, "obsv")?;
    let (p, cc) = shape(c, "obsv")?;
    if cc != n {
        return Err(err(format!(
            "obsv: C must have {n} columns to match A, got {cc}"
        )));
    }
    let mut out = vec![vec![0.0; n]; n * p];
    let mut block = c.clone();
    for i in 0..n {
        for r in 0..p {
            for col in 0..n {
                out[i * p + r][col] = block[r][col];
            }
        }
        block = linalg::mat_mul(&block, a);
    }
    Ok(out)
}

/// State similarity transform `x = P z`. Port of `ControllerDesign.ss2ss`.
pub fn ss2ss(a: &Mat, b: &Mat, c: &Mat, d: &Mat, p: &Mat) -> Result<StateSpaceMatrices> {
    let p_inv = linalg::inverse(p)?;
    Ok(StateSpaceMatrices {
        a: linalg::mat_mul(&linalg::mat_mul(&p_inv, a), p),
        b: linalg::mat_mul(&p_inv, b),
        c: linalg::mat_mul(c, p),
        d: d.clone(),
    })
}

// ---------------------------------------------------------------------------
// Continuous-time LQR (matrix-sign Riccati)
// ---------------------------------------------------------------------------

/// Continuous-time LQR gain `K` (m×n) for `(A, B, Q, R)`. Port of
/// `ControllerDesign.lqr`.
pub fn lqr(a: &Mat, b: &Mat, q: &Mat, r: &Mat) -> Result<Mat> {
    let n = square(a, "lqr")?;
    let (br, _m) = shape(b, "lqr")?;
    if br != n {
        return Err(err(format!(
            "lqr: B must have {n} rows to match A, got {br}"
        )));
    }
    if square(q, "lqr")? != n {
        return Err(err("lqr: Q must be the same size as A"));
    }

    let r_inv = linalg::inverse(r)?;
    let bt = linalg::transpose(b);
    // B R⁻¹ B'  (n×n)
    let brb = linalg::mat_mul(&linalg::mat_mul(b, &r_inv), &bt);

    // Hamiltonian H = [ A , −B R⁻¹ B' ; −Q , −A' ].
    let at = linalg::transpose(a);
    let mut h = vec![vec![0.0; 2 * n]; 2 * n];
    for i in 0..n {
        for j in 0..n {
            h[i][j] = a[i][j];
            h[i][n + j] = -brb[i][j];
            h[n + i][j] = -q[i][j];
            h[n + i][n + j] = -at[i][j];
        }
    }

    let s = matrix_sign(&h)?;
    let sub = |r0: usize, c0: usize| -> Mat {
        (0..n)
            .map(|i| (0..n).map(|j| s[r0 + i][c0 + j]).collect())
            .collect()
    };
    let s11 = sub(0, 0);
    let s12 = sub(0, n);
    let s21 = sub(n, 0);
    let s22 = sub(n, n);
    let eye = linalg::identity(n);

    // Least-squares solve of the overdetermined [S12; S22+I] P = −[S11+I; S21].
    let lhs = stack_rows(&s12, &linalg::add(&s22, &eye));
    let rhs = stack_rows(
        &linalg::scal_mat(&linalg::add(&s11, &eye), -1.0),
        &linalg::scal_mat(&s21, -1.0),
    );
    let lhs_t = linalg::transpose(&lhs);
    let normal = linalg::mat_mul(&lhs_t, &lhs);
    // A singular normal matrix here is not a numerical accident: `[S12; S22+I]`
    // loses rank exactly when the plant has an *unstabilisable* mode, and then
    // no stabilising `P` exists to be found. Diagnose that rather than letting
    // the LU report bare singularity — the Java, whose LU sits on the same
    // `1e-11` pivot threshold, lands on either side of it depending on rounding
    // and *sometimes returns a gain that leaves the uncontrollable pole exactly
    // where it was*. Refusing by name is the behaviour `cas` already commits to.
    let normal_inv = linalg::inverse(&normal).map_err(|e| match unstabilisable_mode(a, b) {
        Some(lambda) => err(format!(
            "lqr: the plant is not stabilisable — the mode at s = {lambda} is \
             uncontrollable and does not decay, so no stabilising gain exists"
        )),
        None => e,
    })?;
    let mut p = linalg::mat_mul(&linalg::mat_mul(&normal_inv, &lhs_t), &rhs);

    // Symmetrise P to scrub numerical asymmetry, then K = R⁻¹ B' P.
    p = linalg::scal_mat(&linalg::add(&p, &linalg::transpose(&p)), 0.5);
    Ok(linalg::mat_mul(&linalg::mat_mul(&r_inv, &bt), &p))
}

/// The non-decaying mode that makes `(A, B)` unstabilisable, printed as its
/// eigenvalue — or `None` when every such mode is reachable.
///
/// Popov–Belevitch–Hautus: a mode `λ` is uncontrollable exactly when
/// `rank[A − λI | B] < n`. Only modes with `Re λ ≥ 0` matter; an uncontrollable
/// mode that decays on its own leaves the plant stabilisable, and LQR solves
/// those — verified against the oracle, which returns the identical gain.
///
/// **Diagnosis only.** This runs after the Riccati solve has already failed, so
/// answering conservatively (`None`, or skipping a complex pair) degrades to
/// the caller's original error and can never turn a failure into a success.
fn unstabilisable_mode(a: &Mat, b: &Mat) -> Option<String> {
    let n = a.len();
    let spectrum = linalg::eigen(a).ok()?;
    for i in 0..n {
        let lambda = *spectrum.re.get(i)?;
        // A decaying mode needs no input. Written negated so a NaN eigenvalue
        // is *not* certified as decaying (the `crate::linalg` convention).
        if !(lambda >= 0.0) {
            continue;
        }
        // A complex pair needs a complex-arithmetic PBH test; not attempted, so
        // the caller keeps its own error rather than getting a guess.
        if spectrum.im.get(i).is_some_and(|im| im.abs() > 0.0) {
            continue;
        }
        let hautus: Mat = (0..n)
            .map(|r| {
                let mut row: Vec<f64> = (0..n)
                    .map(|c| if r == c { a[r][c] - lambda } else { a[r][c] })
                    .collect();
                row.extend(b[r].iter().copied());
                row
            })
            .collect();
        if rank(&hautus).ok()? < n {
            return Some(format!("{lambda}"));
        }
    }
    None
}

/// Newton iteration for the matrix sign function with determinant scaling.
/// Port of the private `ControllerDesign.matrixSign`.
fn matrix_sign(h: &Mat) -> Result<Mat> {
    let mut z = h.clone();
    let dim = z.len();
    for _ in 0..SIGN_MAX_ITERS {
        let z_inv = linalg::inverse(&z)?;
        let det = linalg::det_lu(&z)?;
        let mut c = 1.0;
        if det != 0.0 && det.is_finite() {
            c = det.abs().powf(-1.0 / dim as f64);
        }
        let z_next = linalg::scal_mat(
            &linalg::add(&linalg::scal_mat(&z, c), &linalg::scal_mat(&z_inv, 1.0 / c)),
            0.5,
        );
        let diff = frobenius(&linalg::sub(&z_next, &z));
        z = z_next;
        if diff <= SIGN_TOL * 1.0f64.max(frobenius(&z)) {
            break;
        }
    }
    Ok(z)
}

// ---------------------------------------------------------------------------
// Pole placement (Ackermann)
// ---------------------------------------------------------------------------

/// SISO pole placement via Ackermann's formula:
/// `K = [0 … 0 1]·C⁻¹·φ(A)`. Port of `ControllerDesign.place`.
///
/// `desired_roots` are the requested closed-loop poles as `{re, im}` pairs.
pub fn place(a: &Mat, b: &[f64], desired_roots: &[[f64; 2]]) -> Result<Vec<f64>> {
    let n = square(a, "place")?;
    if b.len() != n {
        return Err(err(format!(
            "place: b must have {n} entries to match A, got {}",
            b.len()
        )));
    }

    // Controllability matrix C = [b, Ab, …, Aⁿ⁻¹b].
    let mut col: Mat = b.iter().map(|v| vec![*v]).collect();
    let mut ctrb_m = vec![vec![0.0; n]; n];
    for j in 0..n {
        for i in 0..n {
            ctrb_m[i][j] = col[i][0];
        }
        col = linalg::mat_mul(a, &col);
    }
    let ctrb_inv = linalg::inverse(&ctrb_m)?;

    // Desired monic characteristic polynomial (descending): [1, c1, …, cn].
    let wanted: Vec<Complex> = desired_roots
        .iter()
        .map(|r| Complex::new(r[0], r[1]))
        .collect();
    let coeffs = tf::expand_roots(&wanted);
    if coeffs.len() != n + 1 {
        return Err(err(format!(
            "place: number of desired poles ({}) must equal the system order n = {n}",
            coeffs.len() - 1
        )));
    }

    // φ(A) = Σ coeffs[k]·A^(n−k).
    let mut powers: Vec<Mat> = Vec::with_capacity(n + 1);
    powers.push(linalg::identity(n));
    for i in 1..=n {
        powers.push(linalg::mat_mul(&powers[i - 1], a));
    }
    let mut phi = vec![vec![0.0; n]; n];
    for k in 0..=n {
        phi = linalg::add(&phi, &linalg::scal_mat(&powers[n - k], coeffs[k]));
    }

    // K = eₙ'·C⁻¹·φ(A).
    let mut last_row = vec![vec![0.0; n]];
    last_row[0][n - 1] = 1.0;
    let k = linalg::mat_mul(&linalg::mat_mul(&last_row, &ctrb_inv), &phi);
    Ok(k[0].clone())
}

// ---------------------------------------------------------------------------
// Loop-shaping PID tuning
// ---------------------------------------------------------------------------

/// Loop-shaping PID auto-tuning with the historical 60° phase-margin default.
/// Port of the 4-argument `ControllerDesign.pidtune`.
pub fn pidtune(num: &[f64], den: &[f64], kind: &str, wc: f64) -> Result<[f64; 3]> {
    pidtune_pm(num, den, kind, wc, 60.0)
}

/// Loop-shaping PID auto-tuning with an explicit target phase margin
/// `pm_deg`. Port of the 5-argument `ControllerDesign.pidtune`.
///
/// At the requested crossover the controller must contribute `Mc∠θc` with
/// `Mc = 1/|G|` and `θc = (−180° + pm) − ∠G`; the gains follow in closed form.
pub fn pidtune_pm(num: &[f64], den: &[f64], kind: &str, wc: f64, pm_deg: f64) -> Result<[f64; 3]> {
    let s = Cm::new(0.0, wc);
    let g = Cm::horner(num, s).divide(Cm::horner(den, s));
    let mg = g.abs();
    if mg == 0.0 || !mg.is_finite() {
        return Err(err(format!(
            "pidtune: plant gain is zero or singular at wc = {wc}"
        )));
    }
    let pg = g.argument(); // plant phase (rad)
    let theta_c = (-core::f64::consts::PI + pm_deg.to_radians()) - pg;
    let mc = 1.0 / mg;

    let (kp, ki, kd);
    match kind {
        "p" => {
            kp = mc;
            ki = 0.0;
            kd = 0.0;
        }
        "pi" => {
            kp = mc * theta_c.cos();
            ki = -wc * mc * theta_c.sin();
            kd = 0.0;
        }
        "pid" => {
            kp = mc * theta_c.cos();
            let q = mc * theta_c.sin();
            kd = (q + (q * q + kp * kp).sqrt()) / (2.0 * wc);
            ki = if kd == 0.0 { 0.0 } else { kp * kp / (4.0 * kd) };
        }
        other => return Err(err(format!("pidtune: unknown controller type '{other}'"))),
    }
    Ok([kp, ki, kd])
}

// ---------------------------------------------------------------------------
// State-space interconnections
// ---------------------------------------------------------------------------

/// Cascade `sys2 ∘ sys1`. Port of `ControllerDesign.ssSeries`.
pub fn ss_series(
    a1: &Mat,
    b1: &Mat,
    c1: &Mat,
    d1: &Mat,
    a2: &Mat,
    b2: &Mat,
    c2: &Mat,
    d2: &Mat,
) -> Result<StateSpaceMatrices> {
    let n1 = a1.len();
    let n2 = a2.len();
    let n = n1 + n2;
    let m = cols(b1, "ssSeries: B1")?;
    let p = c2.len();

    let mut a = vec![vec![0.0; n]; n];
    place_block(&mut a, a1, 0, 0, "ssSeries")?;
    place_block(&mut a, a2, n1, n1, "ssSeries")?;
    if n1 > 0 && n2 > 0 {
        place_block(&mut a, &linalg::mat_mul(b2, c1), n1, 0, "ssSeries")?;
    }

    let mut b = vec![vec![0.0; m]; n];
    place_block(&mut b, b1, 0, 0, "ssSeries")?;
    if n2 > 0 {
        place_block(&mut b, &linalg::mat_mul(b2, d1), n1, 0, "ssSeries")?;
    }

    let mut c = vec![vec![0.0; n]; p];
    if n1 > 0 {
        place_block(&mut c, &linalg::mat_mul(d2, c1), 0, 0, "ssSeries")?;
    }
    place_block(&mut c, c2, 0, n1, "ssSeries")?;

    Ok(StateSpaceMatrices {
        a,
        b,
        c,
        d: linalg::mat_mul(d2, d1),
    })
}

/// Parallel (additive) connection. Port of `ControllerDesign.ssParallel`.
pub fn ss_parallel(
    a1: &Mat,
    b1: &Mat,
    c1: &Mat,
    d1: &Mat,
    a2: &Mat,
    b2: &Mat,
    c2: &Mat,
    d2: &Mat,
) -> Result<StateSpaceMatrices> {
    let n1 = a1.len();
    let n2 = a2.len();
    let n = n1 + n2;
    let m = cols(b1, "ssParallel: B1")?.max(cols(b2, "ssParallel: B2")?);
    let p = c1.len().max(c2.len());

    let mut a = vec![vec![0.0; n]; n];
    place_block(&mut a, a1, 0, 0, "ssParallel")?;
    place_block(&mut a, a2, n1, n1, "ssParallel")?;

    // The Java zero-pads the narrower B / shorter C (`padRight`/`padBottom`)
    // rather than rejecting a width mismatch. `place_block` reproduces that:
    // the destination is sized to the wider of the two, and a narrower source
    // simply leaves the remaining columns at zero.
    let mut b = vec![vec![0.0; m]; n];
    place_block(&mut b, b1, 0, 0, "ssParallel")?;
    place_block(&mut b, b2, n1, 0, "ssParallel")?;

    let mut c = vec![vec![0.0; n]; p];
    place_block(&mut c, c1, 0, 0, "ssParallel")?;
    place_block(&mut c, c2, 0, n1, "ssParallel")?;

    Ok(StateSpaceMatrices {
        a,
        b,
        c,
        d: linalg::add(d1, d2),
    })
}

/// Feedback interconnection; `sign = 1.0` is negative feedback. Port of
/// `ControllerDesign.ssFeedback`.
pub fn ss_feedback(
    a1: &Mat,
    b1: &Mat,
    c1: &Mat,
    d1: &Mat,
    a2: &Mat,
    b2: &Mat,
    c2: &Mat,
    d2: &Mat,
    sign: f64,
) -> Result<StateSpaceMatrices> {
    let n1 = a1.len();
    let n2 = a2.len();
    let p1 = cols(b1, "ssFeedback: B1")?;
    let q1 = c1.len();
    let n = n1 + n2;

    // E = (I + sign·D2·D1)⁻¹.
    let e_inv = linalg::add(
        &linalg::identity(q1),
        &linalg::scal_mat(&linalg::mat_mul(d2, d1), sign),
    );
    let e = linalg::inverse(&e_inv)?;

    let ed2c1 = linalg::mat_mul(&linalg::mat_mul(&e, d2), c1);
    let ec2 = linalg::mat_mul(&e, c2);

    let a11 = linalg::sub(a1, &linalg::scal_mat(&linalg::mat_mul(b1, &ed2c1), sign));
    let a12 = linalg::scal_mat(&linalg::mat_mul(b1, &ec2), -sign);
    let b2d1 = linalg::mat_mul(b2, d1);
    let a21 = linalg::sub(
        &linalg::mat_mul(b2, c1),
        &linalg::scal_mat(&linalg::mat_mul(&b2d1, &ed2c1), sign),
    );
    let a22 = linalg::sub(a2, &linalg::scal_mat(&linalg::mat_mul(&b2d1, &ec2), sign));

    let mut a = vec![vec![0.0; n]; n];
    if n1 > 0 {
        place_block(&mut a, &a11, 0, 0, "ssFeedback")?;
        if n2 > 0 {
            place_block(&mut a, &a12, 0, n1, "ssFeedback")?;
            place_block(&mut a, &a21, n1, 0, "ssFeedback")?;
        }
    }
    if n2 > 0 {
        place_block(&mut a, &a22, n1, n1, "ssFeedback")?;
    }

    let mut b = vec![vec![0.0; p1]; n];
    if n1 > 0 {
        place_block(&mut b, &linalg::mat_mul(b1, &e), 0, 0, "ssFeedback")?;
    }
    if n2 > 0 {
        place_block(&mut b, &linalg::mat_mul(&b2d1, &e), n1, 0, "ssFeedback")?;
    }

    let c11 = linalg::sub(c1, &linalg::scal_mat(&linalg::mat_mul(d1, &ed2c1), sign));
    let c12 = linalg::scal_mat(&linalg::mat_mul(d1, &ec2), -sign);
    let mut c = vec![vec![0.0; n]; q1];
    if n1 > 0 {
        place_block(&mut c, &c11, 0, 0, "ssFeedback")?;
    }
    if n2 > 0 {
        place_block(&mut c, &c12, 0, n1, "ssFeedback")?;
    }

    Ok(StateSpaceMatrices {
        a,
        b,
        c,
        d: linalg::mat_mul(d1, &e),
    })
}

// ---------------------------------------------------------------------------
// Padé delay approximant
// ---------------------------------------------------------------------------

/// `{num, den}` of the order-`order` Padé approximant of `e^(−Td·s)`. Port of
/// `ControllerDesign.pade`.
pub fn pade(td: f64, order: usize) -> Mat {
    let mut num = vec![0.0; order + 1];
    let mut den = vec![0.0; order + 1];
    for i in 0..=order {
        let k = order - i;
        let coeff = factorial(2 * order - k) / (factorial(k) * factorial(order - k));
        let val = coeff * td.powi(k as i32);
        den[i] = val;
        num[i] = if k % 2 == 0 { val } else { -val };
    }
    vec![num, den]
}

fn factorial(n: usize) -> f64 {
    let mut res = 1.0;
    for i in 2..=n {
        res *= i as f64;
    }
    res
}

// ---------------------------------------------------------------------------
// Step-response metrics
// ---------------------------------------------------------------------------

/// `{riseTime, peakTime, settlingTime, overshoot}` of a sampled step response.
/// Port of `ControllerDesign.stepinfo` (10–90 % rise, 2 % settling band).
///
/// # Divergence (hardening)
///
/// The Java only screens `t.length == 0`; a `y` shorter than `t` walks off the
/// end of the array. Here that returns the same all-zero result as the empty
/// case rather than panicking, because an index panic in wasm aborts the
/// module instead of raising an error the caller can report.
pub fn stepinfo(t: &[f64], y: &[f64]) -> [f64; 4] {
    let big = t.len();
    if big == 0 || y.len() < big {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let yfinal = y[big - 1];

    // Peak: the extreme in the direction the response settles.
    let mut ypeak = y[0];
    let mut ipeak = 0;
    if yfinal >= 0.0 {
        for i in 1..big {
            if y[i] > ypeak {
                ypeak = y[i];
                ipeak = i;
            }
        }
    } else {
        for i in 1..big {
            if y[i] < ypeak {
                ypeak = y[i];
                ipeak = i;
            }
        }
    }
    let tp = t[ipeak];

    let mut os = 0.0;
    if yfinal.abs() > 1e-12 {
        os = 100.0 * (ypeak.abs() - yfinal.abs()) / yfinal.abs();
        if os < 0.0 {
            os = 0.0;
        }
    }

    let t10 = find_time_of_value(t, y, 0.1 * yfinal);
    let t90 = find_time_of_value(t, y, 0.9 * yfinal);
    let tr = t90 - t10;

    // Settling: the last sample outside the 2 % band, interpolated forward.
    let limit = 0.02 * yfinal.abs();
    let mut last_outside: Option<usize> = None;
    for i in (0..big).rev() {
        if (y[i] - yfinal).abs() > limit {
            last_outside = Some(i);
            break;
        }
    }
    let ts = match last_outside {
        None => t[0],
        Some(i) if i == big - 1 => t[big - 1],
        Some(i) => {
            let t0 = t[i];
            let t1 = t[i + 1];
            let y0 = (y[i] - yfinal).abs();
            let y1 = (y[i + 1] - yfinal).abs();
            if (y1 - y0).abs() < 1e-12 {
                t1
            } else {
                t0 + (t1 - t0) * (limit - y0) / (y1 - y0)
            }
        }
    };

    [tr, tp, ts, os]
}

/// First crossing time of `target_val`, linearly interpolated. Port of the
/// private `ControllerDesign.findTimeOfValue`.
fn find_time_of_value(t: &[f64], y: &[f64], target_val: f64) -> f64 {
    let big = t.len();
    let rising = y[big - 1] >= 0.0;
    for i in 0..big {
        let hit = if rising {
            y[i] >= target_val
        } else {
            y[i] <= target_val
        };
        if !hit {
            continue;
        }
        if i == 0 {
            return t[0];
        }
        let (t0, t1, y0, y1) = (t[i - 1], t[i], y[i - 1], y[i]);
        if (y1 - y0).abs() < 1e-12 {
            return t0;
        }
        return t0 + (t1 - t0) * (target_val - y0) / (y1 - y0);
    }
    t[big - 1]
}

// ---------------------------------------------------------------------------
// Root locus
// ---------------------------------------------------------------------------

/// Root locus of `1 + K·num/den` over `m_points` logarithmically spaced gains.
/// Port of `ControllerDesign.rlocus`, including its gain schedule
/// (`k[0] = 0`, then `1e-4·kBase … 100·kBase`).
pub fn rlocus(num: &[f64], den: &[f64], m_points: usize) -> Result<RlocusResult> {
    // The Java sizes the pole table `M × (den.length − 1)`, which throws
    // `NegativeArraySizeException` on an empty denominator and otherwise
    // (degree 0) yields empty rows. Reject the first, keep the second.
    if den.is_empty() {
        return Err(err("rlocus: denominator cannot be empty"));
    }
    let max_den = den.iter().fold(0.0f64, |acc, d| acc.max(d.abs()));
    let max_num = num.iter().fold(0.0f64, |acc, n| acc.max(n.abs()));
    let k_base = if max_num > 1e-12 {
        max_den / max_num
    } else {
        1.0
    };

    let mut k = vec![0.0; m_points];
    if m_points > 1 {
        let k_min = 1e-4 * k_base;
        let k_max = 100.0 * k_base;
        for i in 1..m_points {
            let fraction = (i as f64 - 1.0) / (m_points as f64 - 2.0);
            k[i] = k_min * (k_max / k_min).powf(fraction);
        }
    }

    let big_n = den.len() - 1;
    let mut cpr = vec![vec![0.0; big_n]; m_points];
    let mut cpi = vec![vec![0.0; big_n]; m_points];
    let max_degree = (den.len() - 1).max(num.len().saturating_sub(1));
    for i in 0..m_points {
        let ki = k[i];
        let mut coeffs = vec![0.0; max_degree + 1];
        for (j, d) in den.iter().enumerate() {
            coeffs[max_degree - (den.len() - 1) + j] += d;
        }
        for (j, nj) in num.iter().enumerate() {
            coeffs[max_degree - (num.len() - 1) + j] += ki * nj;
        }
        let r = tf::roots(&coeffs)?;
        for j in 0..big_n {
            if j < r.len() {
                cpr[i][j] = r[j].re;
                cpi[i][j] = r[j].im;
            }
        }
    }
    Ok(RlocusResult { k, cpr, cpi })
}

// ---------------------------------------------------------------------------
// Lyapunov and Riccati equations
// ---------------------------------------------------------------------------

/// Continuous Lyapunov solve `A X + X A' + Q = 0`, via the Kronecker system.
/// Port of `ControllerDesign.lyap`.
pub fn lyap(a: &Mat, q: &Mat) -> Result<Mat> {
    let n = square(a, "lyap")?;
    if square(q, "lyap")? != n {
        return Err(err("lyap: Q must be the same size as A"));
    }
    let mut kmat = vec![vec![0.0; n * n]; n * n];
    for j in 0..n {
        for i in 0..n {
            let row = j * n + i;
            for l in 0..n {
                for k in 0..n {
                    let col = l * n + k;
                    let mut val = 0.0;
                    if j == l {
                        val += a[i][k];
                    }
                    if i == k {
                        val += a[j][l];
                    }
                    kmat[row][col] = val;
                }
            }
        }
    }
    solve_kron(&kmat, q, n)
}

/// Discrete Lyapunov solve `A X A' − X + Q = 0`. Port of
/// `ControllerDesign.dlyap`.
pub fn dlyap(a: &Mat, q: &Mat) -> Result<Mat> {
    let n = square(a, "dlyap")?;
    if square(q, "dlyap")? != n {
        return Err(err("dlyap: Q must be the same size as A"));
    }
    let mut kmat = vec![vec![0.0; n * n]; n * n];
    for j in 0..n {
        for i in 0..n {
            let row = j * n + i;
            for l in 0..n {
                for k in 0..n {
                    let col = l * n + k;
                    let mut val = a[j][l] * a[i][k];
                    if j == l && i == k {
                        val -= 1.0;
                    }
                    kmat[row][col] = val;
                }
            }
        }
    }
    solve_kron(&kmat, q, n)
}

/// The shared tail of [`lyap`] / [`dlyap`]: vectorise `−Q` column-major, solve
/// (LU, falling back to the SVD pseudo-inverse), un-vectorise.
fn solve_kron(kmat: &Mat, q: &Mat, n: usize) -> Result<Mat> {
    let mut vec_q = vec![vec![0.0]; n * n];
    for j in 0..n {
        for i in 0..n {
            vec_q[j * n + i][0] = -q[i][j];
        }
    }
    let vec_x = linalg::solve_or_pinv(kmat, &vec_q)?;
    let mut x = vec![vec![0.0; n]; n];
    for j in 0..n {
        for i in 0..n {
            x[i][j] = vec_x[j * n + i][0];
        }
    }
    Ok(x)
}

/// Discrete algebraic Riccati solve, by the stable invariant subspace of the
/// symplectic matrix. Port of `ControllerDesign.dare`.
pub fn dare(a: &Mat, b: &Mat, q: &Mat, r: &Mat) -> Result<Mat> {
    let n = square(a, "dare")?;
    let a_inv = invert_or_pinv(a)?;
    let r_inv = invert_or_pinv(r)?;
    let a_inv_t = linalg::transpose(&a_inv);
    let brb = linalg::mat_mul(&linalg::mat_mul(b, &r_inv), &linalg::transpose(b));
    let a_inv_t_q = linalg::mat_mul(&a_inv_t, q);

    let s11 = linalg::add(a, &linalg::mat_mul(&brb, &a_inv_t_q));
    let s12 = linalg::scal_mat(&linalg::mat_mul(&brb, &a_inv_t), -1.0);
    let s21 = linalg::scal_mat(&a_inv_t_q, -1.0);
    let s22 = a_inv_t;

    let mut s = vec![vec![0.0; 2 * n]; 2 * n];
    place_block(&mut s, &s11, 0, 0, "dare")?;
    place_block(&mut s, &s12, 0, n, "dare")?;
    place_block(&mut s, &s21, n, 0, "dare")?;
    place_block(&mut s, &s22, n, n, "dare")?;

    let eig = linalg::eigen(&s)?;
    let mut v11 = vec![vec![0.0; n]; n];
    let mut v21 = vec![vec![0.0; n]; n];
    let mut count = 0usize;
    for i in 0..2 * n {
        let mag = (eig.re[i] * eig.re[i] + eig.im[i] * eig.im[i]).sqrt();
        if mag < 1.0 && count < n {
            for j in 0..n {
                v11[j][count] = eig.v[j][i];
                v21[j][count] = eig.v[n + j][i];
            }
            count += 1;
        }
    }
    if count < n {
        return Err(err("dare: could not find enough stable eigenvalues"));
    }
    Ok(linalg::mat_mul(&v21, &linalg::inverse(&v11)?))
}

/// Discrete-time LQR gain `K = (R + B'XB)⁻¹B'XA`. Port of
/// `ControllerDesign.dlqr`.
pub fn dlqr(a: &Mat, b: &Mat, q: &Mat, r: &Mat) -> Result<Mat> {
    let x = dare(a, b, q, r)?;
    let bt = linalg::transpose(b);
    let btx = linalg::mat_mul(&bt, &x);
    let temp = linalg::add(r, &linalg::mat_mul(&btx, b));
    Ok(linalg::mat_mul(
        &linalg::mat_mul(&linalg::inverse(&temp)?, &btx),
        a,
    ))
}

// ---------------------------------------------------------------------------
// Estimator gain, gramians, balanced realisation
// ---------------------------------------------------------------------------

/// Continuous Kalman estimator gain `L = P C' R⁻¹` (n×p), by duality with
/// [`lqr`]. Port of `ControllerDesign.lqe`.
pub fn lqe(a: &Mat, g: &Mat, c: &Mat, q: &Mat, r: &Mat) -> Result<Mat> {
    // Process-noise covariance mapped into the state space: G Q G' (n×n).
    let gqg = linalg::mat_mul(&linalg::mat_mul(g, q), &linalg::transpose(g));
    let kd = lqr(&linalg::transpose(a), &linalg::transpose(c), &gqg, r)?;
    Ok(linalg::transpose(&kd))
}

/// Controllability (`'c'`) or observability (`'o'`) gramian via the Lyapunov
/// equation. Port of `ControllerDesign.gramian`.
pub fn gramian(a: &Mat, m: &Mat, kind: char) -> Result<Mat> {
    match kind {
        'c' => {
            let bbt = linalg::mat_mul(m, &linalg::transpose(m));
            lyap(a, &bbt)
        }
        'o' => {
            let ctc = linalg::mat_mul(&linalg::transpose(m), m);
            lyap(&linalg::transpose(a), &ctc)
        }
        other => Err(err(format!(
            "gram: type must be 'c' or 'o' (got '{other}')"
        ))),
    }
}

/// Internally balanced realisation of a stable, minimal `(A, B, C)` (Laub's
/// method). Port of `ControllerDesign.balreal`.
pub fn balreal(a: &Mat, b: &Mat, c: &Mat) -> Result<BalrealResult> {
    let wc = gramian(a, b, 'c')?;
    let wo = gramian(a, c, 'o')?;
    let lc = linalg::cholesky_l(&wc)?;
    let lo = linalg::cholesky_l(&wo)?;

    let h = linalg::mat_mul(&linalg::transpose(&lo), &lc); // Lo' Lc
    let f = linalg::svd(&h)?;
    let n = f.s.len();
    let mut s_inv_sqrt = vec![vec![0.0; n]; n];
    for i in 0..n {
        s_inv_sqrt[i][i] = 1.0 / f.s[i].sqrt();
    }

    let t = linalg::mat_mul(&linalg::mat_mul(&lc, &f.v), &s_inv_sqrt);
    let t_inv = linalg::mat_mul(
        &linalg::mat_mul(&s_inv_sqrt, &linalg::transpose(&f.u)),
        &linalg::transpose(&lo),
    );
    Ok(BalrealResult {
        a: linalg::mat_mul(&linalg::mat_mul(&t_inv, a), &t),
        b: linalg::mat_mul(&t_inv, b),
        c: linalg::mat_mul(c, &t),
    })
}

// ---------------------------------------------------------------------------
// Shared numeric helpers
// ---------------------------------------------------------------------------

fn shape(a: &Mat, what: &str) -> Result<(usize, usize)> {
    if a.is_empty() || a[0].is_empty() {
        return Err(err(format!("{what}: matrix must be non-empty")));
    }
    let n = a[0].len();
    if a.iter().any(|row| row.len() != n) {
        return Err(err(format!("{what}: matrix must be rectangular")));
    }
    Ok((a.len(), n))
}

fn square(a: &Mat, what: &str) -> Result<usize> {
    let (m, n) = shape(a, what)?;
    if m != n {
        return Err(err(format!("{what}: matrix must be square, got {m}x{n}")));
    }
    Ok(n)
}

fn cols(a: &Mat, what: &str) -> Result<usize> {
    if a.is_empty() {
        return Err(err(format!("{what}: matrix must be non-empty")));
    }
    Ok(a[0].len())
}

/// Copy `src` into `dst` with its top-left corner at `(r0, c0)` — Commons
/// Math's `RealMatrix.setSubMatrix`, including its refusal to place a block
/// that does not fit.
///
/// A **narrower** `src` is fine and leaves the rest of the destination at
/// zero; that is how the Java's `ssParallel` zero-pads a mismatched `B`/`C`.
/// A block that would run off the edge is an error rather than a silent
/// truncation: quietly dropping a row of a `D` matrix produces an
/// interconnection that looks well-formed and is not the system the user
/// described.
fn place_block(dst: &mut Mat, src: &Mat, r0: usize, c0: usize, what: &str) -> Result<()> {
    let dst_rows = dst.len();
    let dst_cols = if dst_rows == 0 { 0 } else { dst[0].len() };
    let src_cols = src.iter().map(Vec::len).max().unwrap_or(0);
    if r0 + src.len() > dst_rows || c0 + src_cols > dst_cols {
        return Err(err(format!(
            "{what}: a {}x{src_cols} block does not fit at ({r0}, {c0}) in a \
             {dst_rows}x{dst_cols} matrix — check the (A, B, C, D) dimensions",
            src.len()
        )));
    }
    for (i, row) in src.iter().enumerate() {
        for (j, v) in row.iter().enumerate() {
            dst[r0 + i][c0 + j] = *v;
        }
    }
    Ok(())
}

fn stack_rows(top: &Mat, bottom: &Mat) -> Mat {
    let mut out = top.clone();
    out.extend(bottom.iter().cloned());
    out
}

fn frobenius(a: &Mat) -> f64 {
    a.iter()
        .flat_map(|row| row.iter())
        .map(|v| v * v)
        .sum::<f64>()
        .sqrt()
}

/// LU inverse with the Java's SVD pseudo-inverse fallback for a singular input.
fn invert_or_pinv(a: &Mat) -> Result<Mat> {
    match linalg::inverse(a) {
        Ok(inv) => Ok(inv),
        Err(_) => linalg::pinv(a),
    }
}

/// Commons Math `org.apache.commons.math3.complex.Complex`, transcribed for
/// the two places the Java uses it directly: [`pidtune_pm`]'s plant evaluation
/// and `PidTuner::suggest_wc`'s phase sweep. Its `abs` and `divide` are the
/// *scaled* forms Commons Math ships, which differ in the last bits from a
/// naive `sqrt(r² + i²)` / `(a+bi)/(c+di)`; the `PolynomialHelpers` private
/// record in [`support`] deliberately uses the naive ones, because that is
/// what the Java does there.
#[derive(Clone, Copy)]
pub(crate) struct Cm {
    pub re: f64,
    pub im: f64,
}

impl Cm {
    pub(crate) fn new(re: f64, im: f64) -> Cm {
        Cm { re, im }
    }

    /// Horner evaluation of a real-coefficient polynomial (descending powers).
    pub(crate) fn horner(coeffs: &[f64], s: Cm) -> Cm {
        let mut v = Cm::new(0.0, 0.0);
        for c in coeffs {
            v = Cm::new(v.re * s.re - v.im * s.im + c, v.re * s.im + v.im * s.re);
        }
        v
    }

    /// Commons Math `Complex.abs()` — scaled so `r² + i²` cannot overflow.
    pub(crate) fn abs(self) -> f64 {
        if self.re.is_nan() || self.im.is_nan() {
            return f64::NAN;
        }
        if self.re.is_infinite() || self.im.is_infinite() {
            return f64::INFINITY;
        }
        if self.re.abs() < self.im.abs() {
            if self.im == 0.0 {
                return self.re.abs();
            }
            let q = self.re / self.im;
            self.im.abs() * (1.0 + q * q).sqrt()
        } else {
            if self.re == 0.0 {
                return self.im.abs();
            }
            let q = self.im / self.re;
            self.re.abs() * (1.0 + q * q).sqrt()
        }
    }

    /// Commons Math `Complex.getArgument()`.
    pub(crate) fn argument(self) -> f64 {
        self.im.atan2(self.re)
    }

    /// Commons Math `Complex.divide()` — Smith's algorithm.
    pub(crate) fn divide(self, divisor: Cm) -> Cm {
        let (c, d) = (divisor.re, divisor.im);
        if c == 0.0 && d == 0.0 {
            return Cm::new(f64::NAN, f64::NAN);
        }
        if c.abs() < d.abs() {
            let q = c / d;
            let denominator = c * q + d;
            Cm::new(
                (self.re * q + self.im) / denominator,
                (self.im * q - self.re) / denominator,
            )
        } else {
            let q = d / c;
            let denominator = d * q + c;
            Cm::new(
                (self.im * q + self.re) / denominator,
                (self.im - self.re * q) / denominator,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ground truth in this module comes from `ControllerDesign` running inside
    /// the real Java engine (`tools/golden-dumper/classpath.sh` + a harness
    /// that calls the static methods directly). Values are pasted verbatim, so
    /// a regression shows up as a diff against the oracle rather than against
    /// an expectation someone derived on paper.
    const TOL: f64 = 1e-9;

    fn m(rows: &[&[f64]]) -> Mat {
        rows.iter().map(|r| r.to_vec()).collect()
    }

    fn close(actual: f64, expected: f64, tol: f64, what: &str) {
        let scale = expected.abs().max(1.0);
        assert!(
            (actual - expected).abs() <= tol * scale,
            "{what}: got {actual}, oracle {expected}"
        );
    }

    fn vec_close(actual: &[f64], expected: &[f64], tol: f64, what: &str) {
        assert_eq!(actual.len(), expected.len(), "{what}: length");
        for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
            close(*a, *e, tol, &format!("{what}[{i}]"));
        }
    }

    fn mat_close(actual: &Mat, expected: &[&[f64]], tol: f64, what: &str) {
        assert_eq!(actual.len(), expected.len(), "{what}: row count");
        for (i, row) in actual.iter().enumerate() {
            vec_close(row, expected[i], tol, &format!("{what}[{i}]"));
        }
    }

    fn poles_of(a: &Mat, b: &Mat, k: &Mat) -> Vec<[f64; 2]> {
        let acl = linalg::sub(a, &linalg::mat_mul(b, k));
        tf::pole_ss(&acl)
            .unwrap()
            .into_iter()
            .map(|c| [c.re, c.im])
            .collect()
    }

    fn poles_close(actual: &[[f64; 2]], expected: &[[f64; 2]], what: &str) {
        assert_eq!(actual.len(), expected.len(), "{what}: pole count");
        for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
            close(a[0], e[0], TOL, &format!("{what}[{i}].re"));
            close(a[1], e[1], TOL, &format!("{what}[{i}].im"));
        }
    }

    fn eye(n: usize) -> Mat {
        linalg::identity(n)
    }

    // -- LQR -------------------------------------------------------------

    #[test]
    fn lqr_gains_match_the_java_oracle() {
        mat_close(
            &lqr(&m(&[&[0.0]]), &m(&[&[1.0]]), &m(&[&[1.0]]), &m(&[&[1.0]])).unwrap(),
            &[&[1.0]],
            TOL,
            "lqr_scalar",
        );
        mat_close(
            &lqr(
                &m(&[&[0.0, 1.0], &[0.0, 0.0]]),
                &m(&[&[0.0], &[1.0]]),
                &eye(2),
                &m(&[&[1.0]]),
            )
            .unwrap(),
            &[&[1.0000000000000004, 1.732050807568877]],
            TOL,
            "lqr_double_integrator",
        );
        mat_close(
            &lqr(
                &m(&[&[0.0, 1.0], &[0.0, 0.0]]),
                &m(&[&[0.0], &[1.0]]),
                &m(&[&[10.0, 0.0], &[0.0, 2.0]]),
                &m(&[&[0.5]]),
            )
            .unwrap(),
            &[&[4.472135954999573, 3.5978148798957323]],
            TOL,
            "lqr_q_heavy",
        );
        mat_close(
            &lqr(
                &m(&[&[0.0, 1.0, 0.0], &[0.0, 0.0, 1.0], &[-1.0, -2.0, -3.0]]),
                &m(&[&[0.0], &[0.0], &[1.0]]),
                &eye(3),
                &m(&[&[1.0]]),
            )
            .unwrap(),
            &[&[0.4142135623730954, 0.9607143952896311, 0.45274221316611807]],
            TOL,
            "lqr_three_state",
        );
    }

    #[test]
    fn lqr_handles_an_unstable_plant_and_two_inputs() {
        mat_close(
            &lqr(
                &m(&[&[1.0, 2.0], &[3.0, 4.0]]),
                &m(&[&[1.0], &[0.0]]),
                &m(&[&[2.0, 0.0], &[0.0, 3.0]]),
                &m(&[&[4.0]]),
            )
            .unwrap(),
            &[&[11.177398646500968, 17.013240534974855]],
            1e-8,
            "lqr_unstable",
        );
        mat_close(
            &lqr(
                &m(&[&[0.0, 1.0, 0.0], &[0.0, 0.0, 1.0], &[-2.0, -3.0, -4.0]]),
                &m(&[&[1.0, 0.0], &[0.0, 0.0], &[0.0, 1.0]]),
                &m(&[&[1.0, 0.0, 0.0], &[0.0, 2.0, 0.0], &[0.0, 0.0, 3.0]]),
                &m(&[&[1.0, 0.0], &[0.0, 2.0]]),
            )
            .unwrap(),
            &[
                &[
                    1.1567814102954386,
                    0.47922541456701157,
                    -0.08544848832081543,
                ],
                &[
                    -0.042724244160407716,
                    0.21937834303201303,
                    0.23498594613794072,
                ],
            ],
            TOL,
            "lqr_two_input",
        );
    }

    /// The gain matrix alone proves nothing — a stable/anti-stable subspace
    /// mix-up produces a plausible-looking `K` that does not stabilise. These
    /// are the oracle's **closed-loop eigenvalues** of `A − BK`.
    #[test]
    fn lqr_closed_loop_poles_match_the_oracle() {
        let a = m(&[&[0.0, 1.0], &[0.0, 0.0]]);
        let b = m(&[&[0.0], &[1.0]]);
        let k = lqr(&a, &b, &eye(2), &m(&[&[1.0]])).unwrap();
        poles_close(
            &poles_of(&a, &b, &k),
            &[
                [-0.8660254037844385, 0.5000000000000008],
                [-0.8660254037844385, -0.5000000000000008],
            ],
            "lqr_double_integrator_clpoles",
        );

        let au = m(&[&[1.0, 2.0], &[3.0, 4.0]]);
        let bu = m(&[&[1.0], &[0.0]]);
        let ku = lqr(&au, &bu, &m(&[&[2.0, 0.0], &[0.0, 3.0]]), &m(&[&[4.0]])).unwrap();
        let pu = poles_of(&au, &bu, &ku);
        poles_close(
            &pu,
            &[[-5.37122785330817, 0.0], [-0.8061707931927978, 0.0]],
            "lqr_unstable_clpoles",
        );
        // A(1,2;3,4) has an eigenvalue at +5.37: the design has to move it into
        // the left half-plane, and every closed-loop pole must land there.
        for p in &pu {
            assert!(p[0] < 0.0, "closed-loop pole {p:?} is not stable");
        }

        let at = m(&[&[0.0, 1.0, 0.0], &[0.0, 0.0, 1.0], &[-2.0, -3.0, -4.0]]);
        let bt = m(&[&[1.0, 0.0], &[0.0, 0.0], &[0.0, 1.0]]);
        let kt = lqr(
            &at,
            &bt,
            &m(&[&[1.0, 0.0, 0.0], &[0.0, 2.0, 0.0], &[0.0, 0.0, 3.0]]),
            &m(&[&[1.0, 0.0], &[0.0, 2.0]]),
        )
        .unwrap();
        poles_close(
            &poles_of(&at, &bt, &kt),
            &[
                [-1.0295420457042312, 0.6027816324021703],
                [-1.0295420457042312, -0.6027816324021703],
                [-3.332683265024916, 0.0],
            ],
            "lqr_two_input_clpoles",
        );
    }

    /// Independently of the oracle: the returned `P = (B R⁻¹)⁻¹ … ` implied by
    /// `K` must satisfy the continuous ARE. Checked here as the residual of
    /// `A'P + PA − PBR⁻¹B'P + Q` for the scalar case, where `P = K` exactly.
    #[test]
    fn lqr_scalar_solves_the_algebraic_riccati_equation() {
        // A = 0, B = 1, Q = 1, R = 1 → −P² + 1 = 0 → P = 1 → K = 1.
        let k = lqr(&m(&[&[0.0]]), &m(&[&[1.0]]), &m(&[&[1.0]]), &m(&[&[1.0]])).unwrap();
        let p = k[0][0];
        assert!((-p * p + 1.0).abs() < 1e-9, "ARE residual for P = {p}");
    }

    #[test]
    fn lqr_rejects_mismatched_shapes() {
        assert!(lqr(&m(&[&[0.0, 1.0]]), &m(&[&[1.0]]), &eye(1), &m(&[&[1.0]])).is_err());
        assert!(lqr(
            &m(&[&[0.0, 1.0], &[0.0, 0.0]]),
            &m(&[&[1.0]]),
            &eye(2),
            &m(&[&[1.0]])
        )
        .is_err());
        assert!(lqr(
            &m(&[&[0.0, 1.0], &[0.0, 0.0]]),
            &m(&[&[0.0], &[1.0]]),
            &eye(1),
            &m(&[&[1.0]])
        )
        .is_err());
    }

    // -- Pole placement --------------------------------------------------

    #[test]
    fn place_matches_the_oracle_gains() {
        let a = m(&[&[0.0, 1.0], &[0.0, 0.0]]);
        vec_close(
            &place(&a, &[0.0, 1.0], &[[-2.0, 0.0], [-3.0, 0.0]]).unwrap(),
            &[6.0, 5.0],
            TOL,
            "place_double_integrator",
        );
        vec_close(
            &place(&a, &[0.0, 1.0], &[[-1.0, 2.0], [-1.0, -2.0]]).unwrap(),
            &[5.0, 2.0],
            TOL,
            "place_complex",
        );
        vec_close(
            &place(
                &m(&[&[0.0, 1.0, 0.0], &[0.0, 0.0, 1.0], &[-1.0, -2.0, -3.0]]),
                &[0.0, 0.0, 1.0],
                &[[-1.0, 0.0], [-2.0, 1.0], [-2.0, -1.0]],
            )
            .unwrap(),
            &[4.0, 7.0, 2.0],
            TOL,
            "place_three_state",
        );
        // A non-trivial controllability matrix: A = [[1,2],[3,4]], b = [1,1].
        vec_close(
            &place(
                &m(&[&[1.0, 2.0], &[3.0, 4.0]]),
                &[1.0, 1.0],
                &[[-5.0, 0.0], [-6.0, 0.0]],
            )
            .unwrap(),
            &[0.0, 16.0],
            TOL,
            "place_nontrivial_ctrb",
        );
    }

    /// The point of `place` is *where the poles land*, so check that directly.
    #[test]
    fn place_puts_the_closed_loop_poles_where_they_were_asked_for() {
        let a = m(&[&[1.0, 2.0], &[3.0, 4.0]]);
        let b = m(&[&[1.0], &[1.0]]);
        let k = place(&a, &[1.0, 1.0], &[[-5.0, 0.0], [-6.0, 0.0]]).unwrap();
        let mut got: Vec<f64> = poles_of(&a, &b, &m(&[&k])).iter().map(|p| p[0]).collect();
        got.sort_by(|x, y| x.partial_cmp(y).unwrap());
        vec_close(&got, &[-6.0, -5.0], 1e-9, "placed real poles");

        let ai = m(&[&[0.0, 1.0], &[0.0, 0.0]]);
        let bi = m(&[&[0.0], &[1.0]]);
        let kc = place(&ai, &[0.0, 1.0], &[[-1.0, 2.0], [-1.0, -2.0]]).unwrap();
        let pc = poles_of(&ai, &bi, &m(&[&kc]));
        for p in &pc {
            close(p[0], -1.0, 1e-9, "placed complex pole re");
            close(p[1].abs(), 2.0, 1e-9, "placed complex pole |im|");
        }
    }

    #[test]
    fn place_rejects_a_wrong_pole_count() {
        let e = place(
            &m(&[&[0.0, 1.0], &[0.0, 0.0]]),
            &[0.0, 1.0],
            &[[-2.0, 0.0], [-3.0, 0.0], [-4.0, 0.0]],
        )
        .unwrap_err();
        assert!(e.to_string().contains("must equal the system order"), "{e}");
    }

    // -- Structural properties -------------------------------------------

    #[test]
    fn ctrb_obsv_and_rank_match_the_oracle() {
        let a3 = m(&[&[0.0, 1.0, 0.0], &[0.0, 0.0, 1.0], &[-1.0, -2.0, -3.0]]);
        let b3 = m(&[&[0.0], &[0.0], &[1.0]]);
        let c = ctrb(&a3, &b3).unwrap();
        mat_close(
            &c,
            &[&[0.0, 0.0, 1.0], &[0.0, 1.0, -3.0], &[1.0, -3.0, 7.0]],
            TOL,
            "ctrb_textbook",
        );
        assert_eq!(rank(&c).unwrap(), 3);

        let a2 = m(&[&[0.0, 1.0], &[-2.0, -3.0]]);
        let o = obsv(&a2, &m(&[&[1.0, 0.0]])).unwrap();
        mat_close(&o, &[&[1.0, 0.0], &[0.0, 1.0]], TOL, "obsv_textbook");
        assert_eq!(rank(&o).unwrap(), 2);

        // Multi-input / multi-output block layout.
        mat_close(
            &ctrb(&a2, &eye(2)).unwrap(),
            &[&[1.0, 0.0, 0.0, 1.0], &[0.0, 1.0, -2.0, -3.0]],
            TOL,
            "ctrb_two_input",
        );
        mat_close(
            &obsv(&a2, &eye(2)).unwrap(),
            &[&[1.0, 0.0], &[0.0, 1.0], &[0.0, 1.0], &[-2.0, -3.0]],
            TOL,
            "obsv_two_output",
        );
    }

    #[test]
    fn rank_matches_the_oracle_on_degenerate_matrices() {
        assert_eq!(rank(&m(&[&[1.0, 2.0], &[2.0, 4.0]])).unwrap(), 1);
        assert_eq!(rank(&m(&[&[0.0, 0.0], &[0.0, 0.0]])).unwrap(), 0);
    }

    #[test]
    fn ss2ss_diagonalises_the_textbook_system() {
        let r = ss2ss(
            &m(&[&[-3.0, 1.0], &[1.0, -3.0]]),
            &m(&[&[1.0], &[2.0]]),
            &m(&[&[2.0, 3.0]]),
            &m(&[&[0.0]]),
            &m(&[&[1.0, 1.0], &[1.0, -1.0]]),
        )
        .unwrap();
        mat_close(&r.a, &[&[-2.0, 0.0], &[0.0, -4.0]], TOL, "ss2ss a");
        mat_close(&r.b, &[&[1.5], &[-0.5]], TOL, "ss2ss b");
        mat_close(&r.c, &[&[5.0, -1.0]], TOL, "ss2ss c");
        mat_close(&r.d, &[&[0.0]], TOL, "ss2ss d");
    }

    // -- Interconnections -------------------------------------------------

    #[test]
    fn state_space_interconnections_match_the_oracle() {
        let (a1, b1, c1, d1) = (m(&[&[-2.0]]), m(&[&[1.0]]), m(&[&[1.0]]), m(&[&[1.0]]));
        let (a2, b2, c2, d2) = (m(&[&[-3.0]]), m(&[&[1.0]]), m(&[&[2.0]]), m(&[&[1.0]]));

        let ser = ss_series(&a1, &b1, &c1, &d1, &a2, &b2, &c2, &d2).unwrap();
        mat_close(&ser.a, &[&[-2.0, 0.0], &[1.0, -3.0]], TOL, "series a");
        mat_close(&ser.b, &[&[1.0], &[1.0]], TOL, "series b");
        mat_close(&ser.c, &[&[1.0, 2.0]], TOL, "series c");
        mat_close(&ser.d, &[&[1.0]], TOL, "series d");

        let par = ss_parallel(&a1, &b1, &c1, &d1, &a2, &b2, &c2, &d2).unwrap();
        mat_close(&par.a, &[&[-2.0, 0.0], &[0.0, -3.0]], TOL, "parallel a");
        mat_close(&par.b, &[&[1.0], &[1.0]], TOL, "parallel b");
        mat_close(&par.c, &[&[1.0, 2.0]], TOL, "parallel c");
        mat_close(&par.d, &[&[2.0]], TOL, "parallel d");

        let fdb = ss_feedback(&a1, &b1, &c1, &d1, &a2, &b2, &c2, &d2, 1.0).unwrap();
        mat_close(&fdb.a, &[&[-2.5, -1.0], &[0.5, -4.0]], TOL, "feedback a");
        mat_close(&fdb.b, &[&[0.5], &[0.5]], TOL, "feedback b");
        mat_close(&fdb.c, &[&[0.5, -1.0]], TOL, "feedback c");
        mat_close(&fdb.d, &[&[0.5]], TOL, "feedback d");

        // Positive feedback with no direct feedthrough.
        let zero = m(&[&[0.0]]);
        let pos = ss_feedback(&a1, &b1, &c1, &zero, &a2, &b2, &c2, &zero, -1.0).unwrap();
        mat_close(&pos.a, &[&[-2.0, 2.0], &[1.0, -3.0]], TOL, "feedback+ a");
        mat_close(&pos.b, &[&[1.0], &[0.0]], TOL, "feedback+ b");
        mat_close(&pos.c, &[&[1.0, 0.0]], TOL, "feedback+ c");
        mat_close(&pos.d, &[&[0.0]], TOL, "feedback+ d");
    }

    /// `ssParallel` zero-pads a narrower `B`/`C` (the Java does it explicitly
    /// with `padRight`/`padBottom`), but a block that would run off the edge
    /// of the assembled matrix is an error, matching Commons Math's
    /// `setSubMatrix`. Truncating it silently would hand back an
    /// interconnection that is not the system the user described.
    #[test]
    fn ss_parallel_pads_a_narrow_input_and_refuses_an_oversized_block() {
        // System 1 is single-input, system 2 has two inputs: B1 is padded.
        let par = ss_parallel(
            &m(&[&[-2.0]]),
            &m(&[&[1.0]]),
            &m(&[&[1.0]]),
            &m(&[&[0.0, 0.0]]),
            &m(&[&[-3.0]]),
            &m(&[&[2.0, 3.0]]),
            &m(&[&[4.0]]),
            &m(&[&[0.0, 0.0]]),
        )
        .unwrap();
        mat_close(&par.b, &[&[1.0, 0.0], &[2.0, 3.0]], TOL, "padded B");

        // A C1 wider than the whole assembled state vector cannot be placed.
        assert!(ss_parallel(
            &m(&[&[-2.0]]),
            &m(&[&[1.0]]),
            &m(&[&[1.0, 2.0, 3.0]]),
            &m(&[&[0.0]]),
            &m(&[&[-3.0]]),
            &m(&[&[1.0]]),
            &m(&[&[4.0]]),
            &m(&[&[0.0]]),
        )
        .is_err());
    }

    #[test]
    fn ss_series_composes_unequal_state_counts() {
        let s = ss_series(
            &m(&[&[0.0, 1.0], &[-2.0, -3.0]]),
            &m(&[&[0.0], &[1.0]]),
            &m(&[&[1.0, 0.0]]),
            &m(&[&[0.0]]),
            &m(&[&[-4.0]]),
            &m(&[&[2.0]]),
            &m(&[&[3.0]]),
            &m(&[&[0.5]]),
        )
        .unwrap();
        mat_close(
            &s.a,
            &[&[0.0, 1.0, 0.0], &[-2.0, -3.0, 0.0], &[2.0, 0.0, -4.0]],
            TOL,
            "series2 a",
        );
        mat_close(&s.b, &[&[0.0], &[1.0], &[0.0]], TOL, "series2 b");
        mat_close(&s.c, &[&[0.5, 0.0, 3.0]], TOL, "series2 c");
        mat_close(&s.d, &[&[0.0]], TOL, "series2 d");
    }

    // -- Padé --------------------------------------------------------------

    #[test]
    fn pade_matches_the_oracle() {
        mat_close(
            &pade(0.2, 2),
            &[
                &[0.04000000000000001, -1.2000000000000002, 12.0],
                &[0.04000000000000001, 1.2000000000000002, 12.0],
            ],
            TOL,
            "pade_2",
        );
        mat_close(&pade(0.5, 1), &[&[-0.5, 2.0], &[0.5, 2.0]], TOL, "pade_1");
        mat_close(
            &pade(0.1, 3),
            &[
                &[-0.0010000000000000002, 0.12000000000000002, -6.0, 120.0],
                &[0.0010000000000000002, 0.12000000000000002, 6.0, 120.0],
            ],
            TOL,
            "pade_3",
        );
        mat_close(&pade(0.3, 0), &[&[1.0], &[1.0]], TOL, "pade_0");
    }

    // -- stepinfo ----------------------------------------------------------

    fn underdamped_step() -> (Vec<f64>, Vec<f64>) {
        let (wn, z) = (10.0f64, 0.5f64);
        let wd = wn * (1.0 - z * z).sqrt();
        let t: Vec<f64> = (0..301).map(|i| i as f64 * 0.005).collect();
        let y: Vec<f64> = t
            .iter()
            .map(|ti| {
                1.0 - (-z * wn * ti).exp()
                    * ((wd * ti).cos() + z / (1.0 - z * z).sqrt() * (wd * ti).sin())
            })
            .collect();
        (t, y)
    }

    #[test]
    fn stepinfo_matches_the_oracle() {
        let (t, y) = underdamped_step();
        vec_close(
            &stepinfo(&t, &y),
            &[
                0.1636823147826349,
                0.365,
                0.8028698966451722,
                16.373246531534786,
            ],
            TOL,
            "stepinfo_underdamped",
        );

        // A response that settles NEGATIVE: the peak search flips direction.
        let t: Vec<f64> = (0..201).map(|i| i as f64 * 0.02).collect();
        let y: Vec<f64> = t.iter().map(|ti| -(1.0 - (-3.0 * ti).exp())).collect();
        vec_close(
            &stepinfo(&t, &y),
            &[0.7324209211307467, 4.0, 1.3040022172917378, 0.0],
            TOL,
            "stepinfo_negative",
        );

        // A monotone first-order rise, and the single-sample degenerate case.
        let t: Vec<f64> = (0..101).map(|i| i as f64 * 0.05).collect();
        let y: Vec<f64> = t.iter().map(|ti| 1.0 - (-2.0 * ti).exp()).collect();
        vec_close(
            &stepinfo(&t, &y),
            &[1.0983360338988184, 5.0, 1.9551243920262609, 0.0],
            TOL,
            "stepinfo_first_order",
        );
        vec_close(
            &stepinfo(&[0.0], &[0.0]),
            &[0.0, 0.0, 0.0, 0.0],
            TOL,
            "stepinfo_single_point",
        );
        assert_eq!(stepinfo(&[], &[]), [0.0, 0.0, 0.0, 0.0]);
    }

    // -- Root locus --------------------------------------------------------

    #[test]
    fn rlocus_gain_schedule_matches_the_oracle() {
        let r = rlocus(&[1.0, 3.0], &[1.0, 7.0, 14.0, 8.0, 0.0], 25).unwrap();
        assert_eq!(r.k[0], 0.0);
        close(r.k[1], 4.666666666666667e-4, TOL, "k[1] = 1e-4·kBase");
        close(r.k[24], 466.6666666666667, TOL, "k[24] = 100·kBase");
        close(r.k[12], 0.34559861897224714, TOL, "k[12]");
        assert_eq!((r.cpr.len(), r.cpr[0].len()), (25, 4));
    }

    /// The per-gain root ORDER is user-visible (it is what draws the locus
    /// branches), and it is inherited from the eigen-decomposition. These rows
    /// straddle the real→complex transition, where an ordering slip would show.
    #[test]
    fn rlocus_pole_rows_match_the_oracle_including_their_order() {
        let r = rlocus(&[1.0, 3.0], &[1.0, 7.0, 14.0, 8.0, 0.0], 25).unwrap();
        vec_close(
            &r.cpr[0],
            &[
                -3.999999999999993,
                -2.0000000000000027,
                -0.9999999999999983,
                1.5909968877541653e-17,
            ],
            1e-9,
            "rlocus cpr row 0",
        );
        vec_close(
            &r.cpr[13],
            &[
                -4.026193802876341,
                -2.123397697056146,
                -0.4252042500337547,
                -0.4252042500337547,
            ],
            1e-9,
            "rlocus cpr row 13",
        );
        vec_close(
            &r.cpi[13],
            &[0.0, 0.0, 0.20081288140551484, -0.20081288140551484],
            1e-9,
            "rlocus cpi row 13",
        );
        vec_close(
            &r.cpr[24],
            &[
                2.6238367183392723,
                2.6238367183392723,
                -9.26039259154285,
                -2.987280845135717,
            ],
            1e-8,
            "rlocus cpr row 24",
        );
        vec_close(
            &r.cpi[24],
            &[6.61240334697706, -6.61240334697706, 0.0, 0.0],
            1e-8,
            "rlocus cpi row 24",
        );
    }

    #[test]
    fn rlocus_second_order_branches_leave_the_real_axis() {
        let r = rlocus(&[1.0], &[1.0, 2.0, 1.0], 7).unwrap();
        vec_close(&r.cpr[0], &[-1.0, -1.0], 1e-12, "rlocus2 cpr row 0");
        vec_close(
            &r.cpi[6],
            &[14.142135623730951, -14.142135623730951],
            1e-9,
            "rlocus2 cpi row 6",
        );
    }

    #[test]
    fn rlocus_survives_degenerate_inputs_without_hanging_or_panicking() {
        // Empty denominator: the Java sizes a negative array and throws.
        assert!(rlocus(&[1.0], &[], 5).is_err());
        // Degree-0 denominator: the Java produces M rows of zero poles.
        let r = rlocus(&[1.0], &[5.0], 4).unwrap();
        assert_eq!(r.k.len(), 4);
        assert!(r.cpr.iter().all(|row| row.is_empty()));
        // An empty numerator leaves kBase at 1 and never indexes `num`.
        let r = rlocus(&[], &[1.0, 2.0, 1.0], 3).unwrap();
        assert_eq!((r.cpr.len(), r.cpr[0].len()), (3, 2));
        // M = 2 makes the Java's gain schedule divide 0/0. The NaN gain lands
        // in the characteristic polynomial, and the Java's eigen solver then
        // fails to converge — `IllegalArgumentException: Failed to calculate
        // roots: illegal state: convergence failed`. Refusing it is the
        // behaviour to match; returning poles would be inventing them.
        let e = rlocus(&[1.0], &[1.0, 2.0, 1.0], 2).unwrap_err();
        assert!(e.to_string().contains("non-finite"), "{e}");
        // A single sample is just K = 0.
        let r = rlocus(&[1.0], &[1.0, 2.0, 1.0], 1).unwrap();
        assert_eq!(r.k, vec![0.0]);
    }

    // -- Lyapunov / Riccati ------------------------------------------------

    #[test]
    fn lyap_and_dlyap_match_the_oracle() {
        mat_close(
            &lyap(&m(&[&[-1.0, 0.0], &[0.0, -2.0]]), &eye(2)).unwrap(),
            &[&[0.5, 0.0], &[0.0, 0.25]],
            TOL,
            "lyap_2x2",
        );
        mat_close(
            &lyap(
                &m(&[&[-2.0, 1.0], &[-1.0, -3.0]]),
                &m(&[&[2.0, 0.5], &[0.5, 1.0]]),
            )
            .unwrap(),
            &[
                &[0.5142857142857142, 0.02857142857142857],
                &[0.02857142857142857, 0.15714285714285714],
            ],
            TOL,
            "lyap_coupled",
        );
        mat_close(
            &dlyap(&m(&[&[0.5, 0.0], &[0.0, 0.25]]), &eye(2)).unwrap(),
            &[&[1.3333333333333333, 0.0], &[0.0, 1.0666666666666667]],
            TOL,
            "dlyap_2x2",
        );
        mat_close(
            &dlyap(
                &m(&[&[0.5, 0.2], &[-0.1, 0.4]]),
                &m(&[&[1.0, 0.3], &[0.3, 2.0]]),
            )
            .unwrap(),
            &[
                &[1.5916787614900825, 0.49830672472181914],
                &[0.4983067247218191, 2.352443154329947],
            ],
            TOL,
            "dlyap_coupled",
        );
    }

    #[test]
    fn lyap_solution_satisfies_its_own_equation() {
        let a = m(&[&[-2.0, 1.0], &[-1.0, -3.0]]);
        let q = m(&[&[2.0, 0.5], &[0.5, 1.0]]);
        let x = lyap(&a, &q).unwrap();
        // A X + X A' + Q = 0
        let res = linalg::add(
            &linalg::add(
                &linalg::mat_mul(&a, &x),
                &linalg::mat_mul(&x, &linalg::transpose(&a)),
            ),
            &q,
        );
        for row in &res {
            for v in row {
                assert!(v.abs() < 1e-12, "lyap residual {v}");
            }
        }
    }

    #[test]
    fn dare_and_dlqr_match_the_oracle() {
        mat_close(
            &dare(&m(&[&[0.5]]), &m(&[&[1.0]]), &m(&[&[1.0]]), &m(&[&[1.0]])).unwrap(),
            &[&[1.1327822185373184]],
            TOL,
            "dare_scalar",
        );
        mat_close(
            &dlqr(&m(&[&[0.5]]), &m(&[&[1.0]]), &m(&[&[1.0]]), &m(&[&[1.0]])).unwrap(),
            &[&[0.2655644370746374]],
            TOL,
            "dlqr_scalar",
        );
        let a = m(&[&[1.0, 1.0], &[0.0, 1.0]]);
        let b = m(&[&[0.0], &[1.0]]);
        mat_close(
            &dare(&a, &b, &eye(2), &m(&[&[1.0]])).unwrap(),
            &[
                &[2.947122966707015, 2.3692054070924686],
                &[2.369205407092468, 4.613134260996184],
            ],
            TOL,
            "dare_2x2",
        );
        mat_close(
            &dlqr(&a, &b, &eye(2), &m(&[&[1.0]])).unwrap(),
            &[&[0.42208244038545345, 1.2439288539037137]],
            TOL,
            "dlqr_2x2",
        );
        let ad = m(&[&[0.9, 0.1], &[-0.2, 0.8]]);
        let bd = m(&[&[1.0], &[0.5]]);
        mat_close(
            &dare(&ad, &bd, &m(&[&[2.0, 0.0], &[0.0, 1.0]]), &m(&[&[3.0]])).unwrap(),
            &[
                &[4.233329042098553, -0.921284222228882],
                &[-0.9212842222288904, 2.350306439460305],
            ],
            TOL,
            "dare_damped",
        );
        mat_close(
            &dlqr(&ad, &bd, &m(&[&[2.0, 0.0], &[0.0, 1.0]]), &m(&[&[3.0]])).unwrap(),
            &[&[0.4847576743806984, 0.08411532386375284]],
            TOL,
            "dlqr_damped",
        );
    }

    /// The discrete design is the one that picks an invariant subspace, so
    /// check both the DARE residual and that the closed loop is inside the
    /// unit disc — an anti-stable mix-up fails the second even when `X` looks
    /// symmetric and positive.
    #[test]
    fn dlqr_closed_loop_is_inside_the_unit_disc() {
        let a = m(&[&[1.0, 1.0], &[0.0, 1.0]]);
        let b = m(&[&[0.0], &[1.0]]);
        let q = eye(2);
        let r = m(&[&[1.0]]);
        let x = dare(&a, &b, &q, &r).unwrap();
        // A'XA − X − A'XB(R + B'XB)⁻¹B'XA + Q = 0
        let at = linalg::transpose(&a);
        let bt = linalg::transpose(&b);
        let atxa = linalg::mat_mul(&linalg::mat_mul(&at, &x), &a);
        let atxb = linalg::mat_mul(&linalg::mat_mul(&at, &x), &b);
        let btxa = linalg::mat_mul(&linalg::mat_mul(&bt, &x), &a);
        let mid = linalg::inverse(&linalg::add(
            &r,
            &linalg::mat_mul(&linalg::mat_mul(&bt, &x), &b),
        ))
        .unwrap();
        let res = linalg::add(
            &linalg::sub(
                &linalg::sub(&atxa, &x),
                &linalg::mat_mul(&linalg::mat_mul(&atxb, &mid), &btxa),
            ),
            &q,
        );
        for row in &res {
            for v in row {
                assert!(v.abs() < 1e-9, "DARE residual {v}");
            }
        }

        let k = dlqr(&a, &b, &q, &r).unwrap();
        for p in poles_of(&a, &b, &k) {
            let mag = p[0].hypot(p[1]);
            assert!(mag < 1.0, "closed-loop |z| = {mag} is not inside the disc");
        }
    }

    // -- Estimator, gramians, balanced realisation -------------------------

    #[test]
    fn lqe_and_gramians_match_the_oracle() {
        mat_close(
            &lqe(
                &m(&[&[-1.0, 1.0], &[0.0, -2.0]]),
                &eye(2),
                &m(&[&[1.0, 0.0]]),
                &eye(2),
                &m(&[&[0.5]]),
            )
            .unwrap(),
            &[&[0.805695044738593], &[0.13026729729675496]],
            TOL,
            "lqe_2x2",
        );
        let a = m(&[&[-1.0, 0.0], &[0.0, -2.0]]);
        mat_close(
            &gramian(&a, &m(&[&[1.0], &[1.0]]), 'c').unwrap(),
            &[&[0.5, 0.3333333333333333], &[0.3333333333333333, 0.25]],
            TOL,
            "gram_c",
        );
        mat_close(
            &gramian(&a, &m(&[&[1.0, 1.0]]), 'o').unwrap(),
            &[&[0.5, 0.3333333333333333], &[0.3333333333333333, 0.25]],
            TOL,
            "gram_o",
        );
        mat_close(
            &gramian(
                &m(&[&[-2.0, 1.0], &[-1.0, -3.0]]),
                &m(&[&[1.0], &[2.0]]),
                'c',
            )
            .unwrap(),
            &[
                &[0.4571428571428571, 0.41428571428571426],
                &[0.41428571428571426, 0.5285714285714285],
            ],
            TOL,
            "gram_c_coupled",
        );
        assert!(gramian(&a, &m(&[&[1.0], &[1.0]]), 'x').is_err());
        // A gramian of a stable, reachable system is positive definite, so it
        // must factor — which is what `balreal` then relies on.
        assert!(linalg::cholesky_l(&gramian(&a, &m(&[&[1.0], &[1.0]]), 'c').unwrap()).is_ok());
    }

    /// The estimator gain is only useful if the error dynamics `A − LC` are
    /// stable; the gain matrix alone does not show that.
    #[test]
    fn lqe_produces_a_stable_estimator() {
        let a = m(&[&[-1.0, 1.0], &[0.0, -2.0]]);
        let c = m(&[&[1.0, 0.0]]);
        let l = lqe(&a, &eye(2), &c, &eye(2), &m(&[&[0.5]])).unwrap();
        let acl = linalg::sub(&a, &linalg::mat_mul(&l, &c));
        for p in tf::pole_ss(&acl).unwrap() {
            assert!(p.re < 0.0, "estimator pole {p:?} is not stable");
        }
    }

    /// `balreal` inherits the SVD column-sign convention, which
    /// `crate::linalg` records as a known divergence from Commons Math: a
    /// balanced state may come out negated. What is invariant — and what the
    /// realisation is *for* — is the diagonal of `Ab` (the modal decay rates),
    /// the magnitudes of `Bb`/`Cb`, and `Bb = Cb'` for this symmetric case.
    #[test]
    fn balreal_matches_the_oracle_up_to_a_state_sign() {
        let b = balreal(
            &m(&[&[-1.0, 0.0], &[0.0, -2.0]]),
            &m(&[&[1.0], &[1.0]]),
            &m(&[&[1.0, 1.0]]),
        )
        .unwrap();
        close(b.a[0][0], -1.324438279205804, TOL, "balreal a00");
        close(b.a[1][1], -1.6755617207941955, TOL, "balreal a11");
        close(b.a[0][1].abs(), 0.46816458878452194, TOL, "balreal |a01|");
        close(b.a[0][1], b.a[1][0], 1e-9, "balreal is symmetric here");
        close(b.b[0][0].abs(), 1.3915204553182265, TOL, "balreal |b0|");
        close(b.b[1][0].abs(), 0.2523307797930242, TOL, "balreal |b1|");
        close(b.c[0][0].abs(), 1.3915204553182265, TOL, "balreal |c0|");
        close(b.c[0][1].abs(), 0.25233077979302365, TOL, "balreal |c1|");
        // Same state, same sign: B and C' agree column by column.
        close(b.b[0][0], b.c[0][0], 1e-9, "balreal b0 = c0");
        close(b.b[1][0], b.c[0][1], 1e-9, "balreal b1 = c1");
    }

    /// A balanced realisation must have equal, diagonal gramians — the actual
    /// defining property, checked independently of any sign convention.
    #[test]
    fn balreal_equalises_and_diagonalises_the_gramians() {
        let (a, b, c) = (
            m(&[&[-2.0, 1.0], &[-1.0, -3.0]]),
            m(&[&[1.0], &[2.0]]),
            m(&[&[3.0, 1.0]]),
        );
        let bal = balreal(&a, &b, &c).unwrap();
        let wc = gramian(&bal.a, &bal.b, 'c').unwrap();
        let wo = gramian(&bal.a, &bal.c, 'o').unwrap();
        for i in 0..2 {
            for j in 0..2 {
                close(wc[i][j], wo[i][j], 1e-8, "balanced Wc = Wo");
                if i != j {
                    assert!(wc[i][j].abs() < 1e-8, "off-diagonal Wc {}", wc[i][j]);
                }
            }
        }
        // The Hankel singular values are sign-free, so they compare against
        // the oracle's balanced gramian directly.
        close(wc[0][0], 1.338030003462654, 1e-9, "sigma_1");
        close(wc[1][1], 0.05231571774836782, 1e-9, "sigma_2");
    }

    // -- pidtune -----------------------------------------------------------

    #[test]
    fn pidtune_matches_the_oracle() {
        vec_close(
            &pidtune(&[0.0, 0.0, 1.0], &[1.0, 1.0, 0.0], "pid", 1.0).unwrap(),
            &[1.3660254037844386, 0.5240940792943284, 0.8901194830787665],
            TOL,
            "pidtune_pid",
        );
        vec_close(
            &pidtune(&[0.0, 0.0, 1.0], &[1.0, 1.0, 0.0], "pi", 0.5).unwrap(),
            &[0.5580127018922192, 0.016746824526945234, 0.0],
            TOL,
            "pidtune_pi",
        );
        vec_close(
            &pidtune(&[2.0], &[5.0, 1.0], "p", 0.5).unwrap(),
            &[1.346291201783626, 0.0, 0.0],
            TOL,
            "pidtune_p",
        );
        vec_close(
            &pidtune_pm(&[1.0], &[1.0, 1.0, 0.0], "pid", 1.0, 40.0).unwrap(),
            &[1.4088320528055172, 0.7687351979027667, 0.6454783644703281],
            TOL,
            "pidtune_pid_pm40",
        );
        vec_close(
            &pidtune_pm(&[1.0], &[1.0, 1.0, 0.0], "pid", 1.0, 70.0).unwrap(),
            &[1.2817127641115769, 0.4082705424564276, 1.0059430199166672],
            TOL,
            "pidtune_pid_pm70",
        );
        vec_close(
            &pidtune_pm(&[2.0], &[5.0, 1.0], "pi", 0.5, 60.0).unwrap(),
            &[0.832531754730548, 0.5290063509461097, 0.0],
            TOL,
            "pidtune_pi_firstorder",
        );
    }

    /// The 4-argument entry point is the 5-argument one at 60°.
    #[test]
    fn pidtune_default_phase_margin_is_sixty_degrees() {
        let a = pidtune(&[1.0], &[1.0, 1.0, 0.0], "pid", 1.0).unwrap();
        let b = pidtune_pm(&[1.0], &[1.0, 1.0, 0.0], "pid", 1.0, 60.0).unwrap();
        vec_close(&a, &b, 1e-12, "pidtune default pm");
    }

    /// The design target is a loop margin, so verify it on the realised loop
    /// rather than on the gains.
    #[test]
    fn pidtune_hits_the_requested_crossover_and_margin() {
        let (num, den) = (vec![0.0, 0.0, 1.0], vec![1.0, 1.0, 0.0]);
        let g = pidtune(&num, &den, "pid", 1.0).unwrap();
        let loop_num = tf::multiply_raw(&[g[2], g[0], g[1]], &num);
        let loop_den = tf::multiply_raw(&[1.0, 0.0], &den);
        let mar = tf::margin(&loop_num, &loop_den);
        close(mar[1], 60.0, 0.02, "realised phase margin");
        close(mar[2], 1.0, 0.02, "realised gain crossover");
    }

    /// LQR needs `(A, B)` **stabilisable**, not controllable — and the
    /// difference is observable. Verified against the oracle, which returns the
    /// identical gain on the three stabilisable plants below and, on the
    /// unstabilisable one, a gain that leaves the uncontrollable pole at `+2`
    /// exactly where it was (`A = [2 1; 0 3]`, `B = [1; 1]`, `Q = 10·I`,
    /// `R = 1` → Java `K = [6.923813920608591, 5.17525765006695]`, closed-loop
    /// eigenvalues `{−9.099, +2}`). Refusing by name is the correct answer.
    #[test]
    fn lqr_solves_stabilisable_plants_and_names_the_mode_when_it_cannot() {
        // Uncontrollable but stabilisable: B spans the λ = 3 eigenvector, so
        // λ = −2 is unreachable — and decays on its own. Oracle: identical K.
        let k = lqr(
            &m(&[&[3.0, 1.0], &[0.0, -2.0]]),
            &m(&[&[1.0], &[0.0]]),
            &eye(2),
            &m(&[&[1.0]]),
        )
        .expect("a stabilisable plant has an LQR solution");
        close(k[0][0], 6.16227766016838, 1e-9, "K1");
        close(k[0][1], 1.1937129433869, 1e-9, "K2");

        // Uncontrollable *and* not stabilisable: λ = 2 is unreachable and does
        // not decay. No stabilising gain exists, so it is named, not invented.
        let e = lqr(
            &m(&[&[2.0, 1.0], &[0.0, 3.0]]),
            &m(&[&[1.0], &[1.0]]),
            &m(&[&[10.0, 0.0], &[0.0, 10.0]]),
            &m(&[&[1.0]]),
        )
        .unwrap_err();
        assert!(e.to_string().contains("not stabilisable"), "{e}");
        assert!(e.to_string().contains("s = 2"), "{e}");
    }

    /// Every entry point that can meet a singular or degenerate argument must
    /// come back with an error, never a panic — in wasm a panic aborts the
    /// module rather than surfacing a diagnostic.
    #[test]
    fn singular_and_degenerate_inputs_are_errors_not_panics() {
        // An uncontrollable plant makes the Hamiltonian singular, so the
        // matrix-sign iteration cannot take its first inverse.
        assert!(lqr(&m(&[&[0.0]]), &m(&[&[0.0]]), &m(&[&[1.0]]), &m(&[&[1.0]])).is_err());
        // A singular R has no inverse.
        assert!(lqr(
            &m(&[&[0.0, 1.0], &[0.0, 0.0]]),
            &m(&[&[0.0], &[1.0]]),
            &eye(2),
            &m(&[&[0.0]])
        )
        .is_err());
        // An uncontrollable pair makes Ackermann's C singular.
        assert!(place(&eye(2), &[1.0, 1.0], &[[-1.0, 0.0], [-2.0, 0.0]]).is_err());
        // dare: A = I, B = 0 puts every eigenvalue of the symplectic matrix on
        // the unit circle, so no stable subspace exists.
        let e = dare(&eye(2), &m(&[&[0.0], &[0.0]]), &eye(2), &m(&[&[1.0]])).unwrap_err();
        assert!(e.to_string().contains("stable eigenvalues"), "{e}");
        // An unstable A has no (positive-definite) gramian, so balreal's
        // Cholesky factorisation must refuse it.
        assert!(balreal(&eye(2), &m(&[&[1.0], &[1.0]]), &m(&[&[1.0, 1.0]])).is_err());
        // Non-square / ragged inputs.
        assert!(rank(&Vec::new()).is_err());
        assert!(ctrb(&m(&[&[1.0, 2.0]]), &m(&[&[1.0]])).is_err());
        assert!(obsv(&eye(2), &m(&[&[1.0]])).is_err());
        assert!(lyap(&m(&[&[1.0, 2.0]]), &eye(1)).is_err());
        assert!(ss2ss(
            &eye(2),
            &eye(2),
            &eye(2),
            &eye(2),
            &m(&[&[1.0, 1.0], &[1.0, 1.0]])
        )
        .is_err());
        // stepinfo with a short y: the Java walks off the end of the array.
        assert_eq!(stepinfo(&[0.0, 1.0, 2.0], &[0.0]), [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn pidtune_rejects_an_unknown_type_and_a_singular_plant() {
        let e = pidtune(&[1.0], &[1.0, 1.0], "lead", 1.0).unwrap_err();
        assert!(e.to_string().contains("unknown controller type"), "{e}");
        // |G(jw)| = 0 at every w: a pure zero numerator.
        let e = pidtune(&[0.0], &[1.0, 1.0], "pi", 1.0).unwrap_err();
        assert!(e.to_string().contains("zero or singular"), "{e}");
    }
}
