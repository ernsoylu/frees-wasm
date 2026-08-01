//! Transfer functions and the polynomial utility layer the control suite shares.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/cas/PolynomialHelpers.java`
//! (988 LOC) and `.../cas/TransferFunction.java` (151 LOC).
//!
//! # Coefficient convention
//!
//! Every polynomial here is a `&[f64]` in **descending** powers, array-language
//! style: `[1, 3, 2]` is `s² + 3s + 2`. That is the Java's convention and the
//! DSL's, and it is load-bearing — `tf([1,3],[1,3,2])` in a document hands these
//! arrays straight through.
//!
//! # `roots` and eigenvalue ORDER
//!
//! `PolynomialHelpers.roots` builds a companion matrix and asks Apache Commons
//! Math for its eigenvalues. **The order they come back in is observable** — it
//! is what a document's `pole(...)` / `zero(...)` arrays contain, and
//! [`residue`] groups poles in first-appearance order — so it is reproduced
//! here rather than replaced by "sorted somehow":
//!
//! * Commons Math `EigenDecomposition` first asks `isSymmetric(matrix, false)`.
//!   For a **symmetric** input it runs the tridiagonal/QL path and then sorts
//!   the (necessarily real) eigenvalues into **decreasing** order. Verified
//!   against the oracle: `poleSS(diag(-2, -1))` returns `[-1, -2]`, i.e. *not*
//!   the diagonal order, and `roots([1, 1, -1])` — whose companion matrix
//!   happens to be symmetric — returns `[0.618…, -1.618…]`, the reverse of what
//!   the general path produces.
//! * For a **non-symmetric** input it runs Hessenberg reduction followed by the
//!   Francis double-shift QR (`SchurTransformer`) and reads the eigenvalues off
//!   the quasi-triangular Schur form **top-left to bottom-right**, a 2×2 block
//!   yielding the `+i` member of the conjugate pair first. That is exactly
//!   [`crate::linalg::eigen`], so [`eigenvalues`] is a thin `EigenDecomposition`
//!   wrapper over it rather than a second QR iteration.
//!
//! The one deliberate divergence: the symmetric branch takes its *values* from
//! the same general path instead of transcribing `TriDiagonalTransformer` plus
//! the implicit-QL sweep, then zeroes the imaginary parts and applies the
//! decreasing sort. Both routines are backward stable, so the values agree to a
//! few ulp (measured against the oracle: ≤ 5e-16 absolute) and the **order** —
//! which is what a caller can see — is fixed by the sort, not by the iteration.
//!
//! # Where the numbers stop agreeing: repeated roots
//!
//! A root of multiplicity `m` is conditioned like `eps^(1/m)`, so the companion
//! matrix cannot resolve one better than that in *either* engine. Measured
//! against the oracle:
//!
//! | polynomial | oracle | this port | gap |
//! |---|---|---|---|
//! | `(s+1)²` | two reals at −1 ± 1e-16 | two reals at −1 | 1e-16 |
//! | `s(s+1)²` | a conjugate pair at −1 ± 7.9e-9i | two reals at −1 ± 2.1e-8 | the **shape** differs |
//! | `(s+1)⁴` | pairs at −1 ± 1.5e-4 | pairs at −1 ± 1.6e-4 | 5e-6 |
//! | `(s+1)³` | −1.0000075 and a pair at ±6.45e-6i | −1.0000075 and a pair at ±6.49e-6i | 4e-8 |
//!
//! The third row is the one to watch: [`residue`] clusters poles with a `1e-6`
//! radius, so those `(s+1)³` copies land as **three simple poles** with
//! residues near ±6e9 that cancel — in the Java as much as here, but to
//! *different* six-billionths. Any caller that shows residues for a
//! triple-or-higher pole is displaying noise in both engines; the tests here
//! assert the pole/order structure and leave those residues unasserted.
//!
//! # This module's `expm` is not [`crate::linalg::expm`]
//!
//! The Java has two matrix exponentials with different algorithms, and the ZOH
//! discretisation uses *this* one: scaling-and-squaring with a truncated
//! 20-term Taylor series ([`expm`]). `LinearAlgebra.expm`, ported in
//! [`crate::linalg::expm`], is a [6/6] Padé approximant. They agree only to
//! ~1e-15, which is exactly the sort of difference [`c2d`]'s output carries into
//! a golden fixture, so they stay separate.

// Numerical kernels index parallel arrays (and 2-D `a[i][j]` slices) by the
// same loop variable, mirroring the Java being transcribed. Iterator rewrites
// obscure that correspondence, so the indexed form stays.
#![allow(clippy::needless_range_loop)]
// The sample-time guards in `c2d`/`d2c` are written `!(ts > 0.0)` on purpose:
// the negation makes NaN take the reject branch, which `ts <= 0.0` would not.
// Clippy's `neg_cmp_op_on_partial_ord` exists to catch the *accidental* form;
// here the NaN behaviour is the point (`crate::linalg` carries the same
// allow for the same reason).
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::ast::{BinOp, Expr};
use crate::diag::{FreesError, Result};
use crate::eval::{eval, Scope};
use crate::linalg::Mat;

// ---------------------------------------------------------------------------
// Complex
// ---------------------------------------------------------------------------

/// The complex value type of `PolynomialHelpers` — a transcription of its
/// private `Complex` record, **including** the guards that make it total:
/// [`Complex::divide`] returns `0` rather than a NaN when the divisor's squared
/// modulus underflows below `1e-30`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Complex {
    /// Real part (the Java field `r`).
    pub re: f64,
    /// Imaginary part (the Java field `i`).
    pub im: f64,
}

impl Complex {
    /// The additive identity.
    pub const ZERO: Complex = Complex { re: 0.0, im: 0.0 };

    pub const fn new(re: f64, im: f64) -> Complex {
        Complex { re, im }
    }

    pub fn multiply(self, o: Complex) -> Complex {
        Complex::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }

    pub fn subtract(self, o: Complex) -> Complex {
        Complex::new(self.re - o.re, self.im - o.im)
    }

    pub fn negate(self) -> Complex {
        Complex::new(-self.re, -self.im)
    }

    // Deliberately named methods rather than `std::ops` impls. [`Complex::divide`]
    // returns `0` for a zero divisor instead of a NaN — surprising behaviour to
    // hide behind `/`, and the whole set stays consistent with the Java record's
    // spelling for the sake of the transcription.
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, o: Complex) -> Complex {
        Complex::new(self.re + o.re, self.im + o.im)
    }

    /// `self / o`, with the Java's underflow guard: a divisor whose squared
    /// modulus is below `1e-30` yields `0`, not an infinity or a NaN.
    pub fn divide(self, o: Complex) -> Complex {
        let denom = o.re * o.re + o.im * o.im;
        if denom.abs() < 1e-30 {
            return Complex::ZERO;
        }
        Complex::new(
            (self.re * o.re + self.im * o.im) / denom,
            (self.im * o.re - self.re * o.im) / denom,
        )
    }

    pub fn magnitude(self) -> f64 {
        (self.re * self.re + self.im * self.im).sqrt()
    }
}

fn err(message: impl Into<String>) -> FreesError {
    FreesError::evaluation(message)
}

// ---------------------------------------------------------------------------
// Polynomial arithmetic
// ---------------------------------------------------------------------------

/// Drops leading coefficients whose magnitude is at or below `1e-15`.
///
/// Port of the private `trimLeadingZeros`. Two behaviours the callers depend
/// on: an all-zero (or empty) input becomes the one-element polynomial `[0.0]`,
/// and the threshold is **absolute**, matching the Java exactly.
pub fn trim_leading_zeros(p: &[f64]) -> Vec<f64> {
    match p.iter().position(|v| v.abs() > 1e-15) {
        None => vec![0.0],
        Some(first) => p[first..].to_vec(),
    }
}

/// `p1 + p2`, right-aligned on the constant term, trimmed. Port of `add`.
pub fn add(p1: &[f64], p2: &[f64]) -> Vec<f64> {
    trim_leading_zeros(&add_raw(p1, p2))
}

/// `p1 · p2`, trimmed. Port of `multiply`.
///
/// An empty operand yields an empty product, exactly as the Java does — the
/// only place in this module where a zero-length polynomial escapes.
pub fn multiply(p1: &[f64], p2: &[f64]) -> Vec<f64> {
    let raw = multiply_raw(p1, p2);
    if raw.is_empty() {
        return raw;
    }
    trim_leading_zeros(&raw)
}

/// `p1 · p2` **without** trimming — the width-preserving form the block-algebra
/// helpers use so `series`/`parallel`/`feedback` keep aligned degrees. Port of
/// `multiplyRaw`.
pub fn multiply_raw(p1: &[f64], p2: &[f64]) -> Vec<f64> {
    if p1.is_empty() || p2.is_empty() {
        return Vec::new();
    }
    let mut result = vec![0.0; p1.len() + p2.len() - 1];
    for i in 0..p1.len() {
        for j in 0..p2.len() {
            result[i + j] += p1[i] * p2[j];
        }
    }
    result
}

/// `p1 + p2` **without** trimming. Port of `addRaw`.
pub fn add_raw(p1: &[f64], p2: &[f64]) -> Vec<f64> {
    let max_len = p1.len().max(p2.len());
    let mut result = vec![0.0; max_len];
    for i in 0..max_len {
        let c1 = if i + p1.len() < max_len {
            0.0
        } else {
            p1[i + p1.len() - max_len]
        };
        let c2 = if i + p2.len() < max_len {
            0.0
        } else {
            p2[i + p2.len() - max_len]
        };
        result[i] = c1 + c2;
    }
    result
}

/// Cascade of two transfer functions: `(num1·num2) / (den1·den2)`.
/// Port of `series`.
pub fn series(num1: &[f64], den1: &[f64], num2: &[f64], den2: &[f64]) -> (Vec<f64>, Vec<f64>) {
    (multiply_raw(num1, num2), multiply_raw(den1, den2))
}

/// Sum of two transfer functions. Port of `parallel`.
pub fn parallel(num1: &[f64], den1: &[f64], num2: &[f64], den2: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let num = add_raw(&multiply_raw(num1, den2), &multiply_raw(num2, den1));
    (num, multiply_raw(den1, den2))
}

/// Closed loop `G1 / (1 + sign·G1·G2)`; `sign = +1` is negative feedback.
/// Port of `feedback`.
pub fn feedback(
    num1: &[f64],
    den1: &[f64],
    num2: &[f64],
    den2: &[f64],
    sign: f64,
) -> (Vec<f64>, Vec<f64>) {
    let num = multiply_raw(num1, den2);
    let mut term2 = multiply_raw(num1, num2);
    for v in &mut term2 {
        *v *= sign;
    }
    let den = add_raw(&multiply_raw(den1, den2), &term2);
    (num, den)
}

/// `p(s)` by Horner's rule over the complex plane. Port of `evalPoly`.
pub fn eval_poly(coeffs: &[f64], s: Complex) -> Complex {
    let mut val = Complex::ZERO;
    for &c in coeffs {
        val = val.multiply(s).add(Complex::new(c, 0.0));
    }
    val
}

// ---------------------------------------------------------------------------
// Roots, and the eigen-decomposition behind them
// ---------------------------------------------------------------------------

/// Commons Math `Precision.EPSILON` — `2^-53`, the unit used by
/// `EigenDecomposition.isSymmetric`'s dimension-scaled tolerance.
const PRECISION_EPSILON: f64 = 1.110_223_024_625_156_5e-16;

/// Roots of a polynomial given in descending powers, via the companion matrix.
///
/// Port of `roots`. Degree 0 (including the all-zero and empty inputs, which
/// [`trim_leading_zeros`] collapses to `[0.0]`) has no roots; degree 1 is solved
/// in closed form without touching the eigen solver, exactly as the Java does.
pub fn roots(coeffs: &[f64]) -> Result<Vec<Complex>> {
    let c = trim_leading_zeros(coeffs);
    if c.len() <= 1 {
        return Ok(Vec::new());
    }
    let degree = c.len() - 1;
    if degree == 1 {
        return Ok(vec![Complex::new(-c[1] / c[0], 0.0)]);
    }
    let mut matrix = vec![vec![0.0; degree]; degree];
    for j in 0..degree {
        matrix[0][j] = -c[j + 1] / c[0];
    }
    for i in 1..degree {
        matrix[i][i - 1] = 1.0;
    }
    eigenvalues(&matrix).map_err(|e| err(format!("Failed to calculate roots: {e}")))
}

/// Eigenvalues of `A` — the poles of a state-space model. Port of `poleSS`.
pub fn pole_ss(a: &Mat) -> Result<Vec<Complex>> {
    eigenvalues(a).map_err(|e| err(format!("Failed to calculate eigenvalues of A: {e}")))
}

/// Eigenvalues of a dense real matrix, in Commons Math `EigenDecomposition`
/// order. See the module docs for why the order is reproduced rather than
/// normalised.
pub fn eigenvalues(a: &Mat) -> Result<Vec<Complex>> {
    let e = crate::linalg::eigen(a)?;
    let mut out: Vec<Complex> = (0..e.re.len())
        .map(|i| Complex::new(e.re[i], e.im[i]))
        .collect();
    if is_symmetric(a) {
        // The symmetric branch of `EigenDecomposition` sorts into decreasing
        // order and cannot produce an imaginary part.
        let mut real: Vec<f64> = out.iter().map(|c| c.re).collect();
        real.sort_by(|x, y| y.total_cmp(x));
        out = real.into_iter().map(|v| Complex::new(v, 0.0)).collect();
    }
    Ok(out)
}

/// `EigenDecomposition.isSymmetric(matrix, false)` — a *relative* test with a
/// dimension-scaled tolerance, so which branch a matrix takes depends on its
/// size as well as on its entries.
fn is_symmetric(a: &Mat) -> bool {
    let rows = a.len();
    let cols = if rows == 0 { 0 } else { a[0].len() };
    let eps = 10.0 * rows as f64 * cols as f64 * PRECISION_EPSILON;
    for i in 0..rows {
        for j in (i + 1)..cols {
            let mij = a[i][j];
            let mji = a[j][i];
            if (mij - mji).abs() > mij.abs().max(mji.abs()) * eps {
                return false;
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Roots ↔ coefficients
// ---------------------------------------------------------------------------

/// Expands complex roots back into a real **monic** polynomial in descending
/// powers. Port of `expandRoots`, including the imaginary parts being discarded
/// as numerical noise and the signed zero the Java's `negate()` produces.
pub fn expand_roots(roots: &[Complex]) -> Vec<f64> {
    if roots.is_empty() {
        return vec![1.0];
    }
    let mut poly = vec![Complex::new(1.0, 0.0)];
    for r in roots {
        let mut next = vec![Complex::ZERO; poly.len() + 1];
        next[0] = poly[0];
        for i in 1..poly.len() {
            next[i] = poly[i].subtract(poly[i - 1].multiply(*r));
        }
        next[poly.len()] = poly[poly.len() - 1].multiply(*r).negate();
        poly = next;
    }
    let result: Vec<f64> = poly.iter().map(|c| c.re).collect();
    trim_leading_zeros(&result)
}

/// Zero-pole-gain model → `(num, den)` coefficients. Port of `zp2tf`.
///
/// Both arrays come back with length `np + 1`; a numerator shorter than that is
/// right-aligned, and one *longer* has its high-order terms dropped — which is
/// what the Java's bounds test does rather than an error.
///
/// The Java sizes each root list from its *real* part alone and indexes the
/// imaginary one blindly; a short imaginary array throws there. Here the
/// shorter of the two wins, which is identical for every well-formed call.
pub fn zp2tf(z_r: &[f64], z_i: &[f64], p_r: &[f64], p_i: &[f64], k: f64) -> (Vec<f64>, Vec<f64>) {
    let nz = z_r.len().min(z_i.len());
    let np = p_r.len().min(p_i.len());
    let z_roots: Vec<Complex> = (0..nz).map(|i| Complex::new(z_r[i], z_i[i])).collect();
    let p_roots: Vec<Complex> = (0..np).map(|i| Complex::new(p_r[i], p_i[i])).collect();
    let mut z_poly = expand_roots(&z_roots);
    let p_poly = expand_roots(&p_roots);
    for v in &mut z_poly {
        *v *= k;
    }

    let mut num = vec![0.0; np + 1];
    let pad = (np + 1) as isize - z_poly.len() as isize;
    for (i, v) in z_poly.iter().enumerate() {
        let dst = i as isize + pad;
        if dst >= 0 && (dst as usize) < num.len() {
            num[dst as usize] = *v;
        }
    }

    let mut den = vec![0.0; np + 1];
    let d_pad = (np + 1) as isize - p_poly.len() as isize;
    for (i, v) in p_poly.iter().enumerate() {
        let dst = i as isize + d_pad;
        if dst >= 0 && (dst as usize) < den.len() {
            den[dst as usize] = *v;
        }
    }
    (num, den)
}

/// Zeros, poles and gain of `num/den`. Port of the `ZpkResult` record.
#[derive(Debug, Clone, PartialEq)]
pub struct Zpk {
    pub zeros: Vec<Complex>,
    pub poles: Vec<Complex>,
    pub k: f64,
}

/// Transfer function → zero-pole-gain. Port of `tf2zp`.
pub fn tf2zp(num: &[f64], den: &[f64]) -> Result<Zpk> {
    let trimmed_num = trim_leading_zeros(num);
    let trimmed_den = trim_leading_zeros(den);
    if trimmed_den.len() == 1 && trimmed_den[0].abs() < 1e-15 {
        return Err(err("tf2zp: denominator cannot be zero"));
    }
    if trimmed_num.len() == 1 && trimmed_num[0].abs() < 1e-15 {
        return Ok(Zpk {
            zeros: Vec::new(),
            poles: roots(&trimmed_den)?,
            k: 0.0,
        });
    }
    let k = trimmed_num[0] / trimmed_den[0];
    Ok(Zpk {
        zeros: roots(&trimmed_num)?,
        poles: roots(&trimmed_den)?,
        k,
    })
}

// ---------------------------------------------------------------------------
// Frequency response
// ---------------------------------------------------------------------------

/// Removes 2π jumps from a phase sequence. Port of `unwrap`.
pub fn unwrap(phase_rad: &[f64]) -> Vec<f64> {
    let n = phase_rad.len();
    let mut unwrapped = vec![0.0; n];
    if n == 0 {
        return unwrapped;
    }
    unwrapped[0] = phase_rad[0];
    let mut offset = 0.0;
    for i in 1..n {
        let diff = phase_rad[i] - phase_rad[i - 1];
        if diff > core::f64::consts::PI {
            offset -= 2.0 * core::f64::consts::PI;
        } else if diff < -core::f64::consts::PI {
            offset += 2.0 * core::f64::consts::PI;
        }
        unwrapped[i] = phase_rad[i] + offset;
    }
    unwrapped
}

/// Bode magnitude (dB) and phase (degrees, unwrapped) at each `omega`.
/// Port of `bode`.
pub fn bode(num: &[f64], den: &[f64], omega: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = omega.len();
    let mut mag = vec![0.0; n];
    let mut phase_rad = vec![0.0; n];
    for i in 0..n {
        let s = Complex::new(0.0, omega[i]);
        let resp = eval_poly(num, s).divide(eval_poly(den, s));
        mag[i] = 20.0 * libm::log10(resp.magnitude().max(1e-30));
        phase_rad[i] = libm::atan2(resp.im, resp.re);
    }
    let phase = unwrap(&phase_rad)
        .into_iter()
        .map(|p| p * (180.0 / core::f64::consts::PI))
        .collect();
    (mag, phase)
}

/// Real and imaginary parts of `G(jω)` at each `omega`. Port of `nyquist`.
pub fn nyquist(num: &[f64], den: &[f64], omega: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = omega.len();
    let mut real = vec![0.0; n];
    let mut imag = vec![0.0; n];
    for i in 0..n {
        let s = Complex::new(0.0, omega[i]);
        let resp = eval_poly(num, s).divide(eval_poly(den, s));
        real[i] = resp.re;
        imag[i] = resp.im;
    }
    (real, imag)
}

/// Gain margin (dB), phase margin (deg), gain-crossover ω and phase-crossover ω.
///
/// Port of `margin`, including its fixed 2 000-point logarithmic sweep over
/// `1e-5 .. 1e5` rad/s and the `1e9` sentinel for "no crossing found". Those
/// constants set the answer's precision, so they are transcribed, not tuned.
pub fn margin(num: &[f64], den: &[f64]) -> [f64; 4] {
    const NUM_POINTS: usize = 2000;
    let w_min = 1e-5f64;
    let w_max = 1e5f64;
    let log_min = libm::log(w_min);
    let log_max = libm::log(w_max);
    let step = (log_max - log_min) / (NUM_POINTS - 1) as f64;

    let mut w = vec![0.0; NUM_POINTS];
    let mut mag = vec![0.0; NUM_POINTS];
    let mut phase = vec![0.0; NUM_POINTS];
    for i in 0..NUM_POINTS {
        w[i] = libm::exp(log_min + i as f64 * step);
        let s = Complex::new(0.0, w[i]);
        let resp = eval_poly(num, s).divide(eval_poly(den, s));
        mag[i] = resp.magnitude();
        phase[i] = libm::atan2(resp.im, resp.re);
    }
    let phase_unwrapped = unwrap(&phase);

    let mut w_cg = 0.0;
    let mut pm = 1e9;
    let mut has_wcg = false;
    let mut w_cp = 0.0;
    let mut gm_db = 1e9;
    let mut has_wcp = false;

    for i in 1..NUM_POINTS {
        if !has_wcg && ((mag[i - 1] >= 1.0 && mag[i] < 1.0) || (mag[i - 1] < 1.0 && mag[i] >= 1.0))
        {
            let r = (1.0 - mag[i - 1]) / (mag[i] - mag[i - 1]);
            let log_wcg = libm::log(w[i - 1]) + r * (libm::log(w[i]) - libm::log(w[i - 1]));
            w_cg = libm::exp(log_wcg);
            let phase_cg =
                phase_unwrapped[i - 1] + r * (phase_unwrapped[i] - phase_unwrapped[i - 1]);
            let phase_cg_deg = phase_cg * (180.0 / core::f64::consts::PI);
            pm = 180.0 + phase_cg_deg;
            while pm <= -180.0 {
                pm += 360.0;
            }
            while pm > 180.0 {
                pm -= 360.0;
            }
            has_wcg = true;
        }

        let target = -core::f64::consts::PI;
        if !has_wcp
            && ((phase_unwrapped[i - 1] >= target && phase_unwrapped[i] < target)
                || (phase_unwrapped[i - 1] < target && phase_unwrapped[i] >= target))
        {
            let r =
                (target - phase_unwrapped[i - 1]) / (phase_unwrapped[i] - phase_unwrapped[i - 1]);
            let log_wcp = libm::log(w[i - 1]) + r * (libm::log(w[i]) - libm::log(w[i - 1]));
            w_cp = libm::exp(log_wcp);
            let mag_cp = mag[i - 1] + r * (mag[i] - mag[i - 1]);
            gm_db = if mag_cp > 1e-30 {
                -20.0 * libm::log10(mag_cp)
            } else {
                1e9
            };
            has_wcp = true;
        }
    }
    [gm_db, pm, w_cg, w_cp]
}

// ---------------------------------------------------------------------------
// Routh–Hurwitz
// ---------------------------------------------------------------------------

/// Numerical tolerance used by the Routh–Hurwitz array.
const ROUTH_EPS: f64 = 1e-12;

/// Number of closed-loop poles in the right half-plane — the sign changes in
/// the first column of the Routh array; `0` means stable. Port of `routh`,
/// including both textbook special cases (the ε method for a zero pivot, and
/// the auxiliary-polynomial derivative for a zero row).
pub fn routh(den: &[f64]) -> usize {
    let c = trim_leading_zeros(den);
    let n = c.len() - 1;
    if n < 1 {
        return 0;
    }
    let cols = n / 2 + 1;
    let mut r = vec![vec![0.0; cols]; n + 1];
    for i in 0..=n {
        r[i % 2][i / 2] = c[i];
    }
    for k in 2..=n {
        if is_zero_row(&r[k - 1]) {
            // The auxiliary polynomial is row k-2, whose highest power is
            // s^(n-(k-2)); its derivative replaces the zero row.
            let power = n as isize - (k as isize - 2);
            for col in 0..cols {
                r[k - 1][col] = r[k - 2][col] * (power - 2 * col as isize) as f64;
            }
        }
        let mut pivot = r[k - 1][0];
        if pivot.abs() < ROUTH_EPS {
            pivot = ROUTH_EPS;
            r[k - 1][0] = pivot;
        }
        for col in 0..(cols - 1) {
            let above_first = r[k - 2][0];
            let above = r[k - 2][col + 1];
            let below_next = r[k - 1][col + 1];
            r[k][col] = (pivot * above - above_first * below_next) / pivot;
        }
    }
    let mut sign_changes = 0;
    let mut prev = routh_sign(r[0][0]);
    for k in 1..=n {
        let s = routh_sign(r[k][0]);
        if s != prev {
            sign_changes += 1;
        }
        prev = s;
    }
    sign_changes
}

fn is_zero_row(row: &[f64]) -> bool {
    !row.iter().any(|v| v.abs() > ROUTH_EPS)
}

/// Sign for Routh first-column counting; a (near-)zero counts as +ε.
fn routh_sign(x: f64) -> f64 {
    if x < -ROUTH_EPS {
        -1.0
    } else {
        1.0
    }
}

// ---------------------------------------------------------------------------
// Discretisation
// ---------------------------------------------------------------------------

const METHOD_TUSTIN: &str = "tustin";
const METHOD_BILINEAR: &str = "bilinear";
const METHOD_ZOH: &str = "zoh";

/// Continuous → discrete. `method` is `"tustin"` (alias `"bilinear"`) or
/// `"zoh"`; `None` means Tustin, matching the Java's null default. Returns
/// `(numz, denz)` in descending powers of `z`, normalised to a monic
/// denominator. Port of `c2d`.
pub fn c2d(
    num: &[f64],
    den: &[f64],
    ts: f64,
    method: Option<&str>,
) -> Result<(Vec<f64>, Vec<f64>)> {
    // Negated comparison on purpose: NaN must take the reject branch.
    if !(ts > 0.0) {
        return Err(err("c2d: sample time Ts must be positive"));
    }
    let nd = trim_leading_zeros(den);
    let nn = trim_leading_zeros(num);
    let n = nd.len() - 1;
    if nn.len() - 1 > n {
        return Err(err(
            "c2d: improper transfer function (numerator degree > denominator degree)",
        ));
    }
    let m = method.unwrap_or(METHOD_TUSTIN).to_ascii_lowercase();
    match m.as_str() {
        METHOD_TUSTIN | METHOD_BILINEAR => {
            let cc = 2.0 / ts;
            let top = [cc, -cc]; // c·(z - 1)
            let bot = [1.0, 1.0]; // (z + 1)
            Ok(normalize_pair(
                substitute_linear_fraction(&nn, n, &top, &bot)?,
                substitute_linear_fraction(&nd, n, &top, &bot)?,
            ))
        }
        METHOD_ZOH => c2d_zoh(&nn, &nd, ts),
        other => Err(err(format!(
            "c2d: unknown method '{other}' (use 'tustin' or 'zoh')"
        ))),
    }
}

/// Discrete → continuous by the inverse Tustin transform. Port of `d2c`.
pub fn d2c(
    numz: &[f64],
    denz: &[f64],
    ts: f64,
    method: Option<&str>,
) -> Result<(Vec<f64>, Vec<f64>)> {
    if !(ts > 0.0) {
        return Err(err("d2c: sample time Ts must be positive"));
    }
    let m = method.unwrap_or(METHOD_TUSTIN).to_ascii_lowercase();
    if m != METHOD_TUSTIN && m != METHOD_BILINEAR {
        return Err(err("d2c: only the 'tustin' method is supported"));
    }
    let nd = trim_leading_zeros(denz);
    let nn = trim_leading_zeros(numz);
    let n = nd.len() - 1;
    let cc = 2.0 / ts;
    let top = [1.0, cc]; // (s + c)
    let bot = [-1.0, cc]; // (c - s)
    Ok(normalize_pair(
        substitute_linear_fraction(&nn, n, &top, &bot)?,
        substitute_linear_fraction(&nd, n, &top, &bot)?,
    ))
}

/// Substitutes the variable of `coeffs` by the linear fraction `top/bot` and
/// clears the denominator by multiplying through by `bot^ref_degree`.
/// Port of `substituteLinearFraction`.
pub fn substitute_linear_fraction(
    coeffs: &[f64],
    ref_degree: usize,
    top: &[f64],
    bot: &[f64],
) -> Result<Vec<f64>> {
    let mut result = vec![0.0; ref_degree + 1];
    let len = coeffs.len();
    for i in 0..len {
        let deg = len - 1 - i;
        if deg > ref_degree {
            return Err(err(
                "substituteLinearFraction: term degree exceeds reference degree",
            ));
        }
        let coeff = coeffs[i];
        // Exactly zero, as in the Java: a 1e-300 coefficient still contributes.
        if coeff == 0.0 {
            continue;
        }
        let term = multiply_raw(&poly_pow(top, deg), &poly_pow(bot, ref_degree - deg));
        // Degree-1 `top`/`bot` (the only shapes `c2d`/`d2c` pass) make `term`
        // exactly `ref_degree + 1` long. A caller passing something wider would
        // walk off the front of `result`; the Java throws
        // ArrayIndexOutOfBounds there, this names the fault.
        if term.len() > result.len() {
            return Err(err(
                "substituteLinearFraction: top and bot must be degree-1 polynomials",
            ));
        }
        let offset = result.len() - term.len();
        for j in 0..term.len() {
            result[offset + j] += coeff * term[j];
        }
    }
    Ok(result)
}

fn poly_pow(base: &[f64], exp: usize) -> Vec<f64> {
    let mut r = vec![1.0];
    for _ in 0..exp {
        r = multiply_raw(&r, base);
    }
    r
}

fn normalize_pair(mut num: Vec<f64>, mut den: Vec<f64>) -> (Vec<f64>, Vec<f64>) {
    let lead = den.first().copied().unwrap_or(0.0);
    if lead.abs() > 1e-15 {
        for v in &mut num {
            *v /= lead;
        }
        for v in &mut den {
            *v /= lead;
        }
    }
    (num, den)
}

fn c2d_zoh(num: &[f64], den: &[f64], ts: f64) -> Result<(Vec<f64>, Vec<f64>)> {
    // tf2ss expects num and den of equal length (n+1); left-pad the numerator.
    let mut num_padded = num.to_vec();
    if num.len() < den.len() {
        num_padded = vec![0.0; den.len()];
        let start = den.len() - num.len();
        num_padded[start..].copy_from_slice(num);
    }
    let ss = super::ss::tf2ss(&num_padded, den)?;
    let n = ss.a.len();
    let b: Vec<f64> = (0..n).map(|i| ss.b[i][0]).collect();
    let cvec: Vec<f64> = if n > 0 { ss.c[0].clone() } else { Vec::new() };
    let d = ss.d[0][0];

    // Augmented matrix M = [[A, B], [0, 0]]; expm(M·Ts) = [[Ad, Bd], [0, 1]].
    let mut m = vec![vec![0.0; n + 1]; n + 1];
    for i in 0..n {
        m[i][..n].copy_from_slice(&ss.a[i][..n]);
        m[i][n] = b[i];
    }
    let scaled: Mat = m
        .iter()
        .map(|row| row.iter().map(|v| v * ts).collect())
        .collect();
    let em = expm(&scaled);
    let mut ad = vec![vec![0.0; n]; n];
    let mut bd = vec![vec![0.0; 1]; n];
    for i in 0..n {
        ad[i][..n].copy_from_slice(&em[i][..n]);
        bd[i][0] = em[i][n];
    }
    let cmat = vec![cvec];
    let tc = super::ss::ss2tf(&ad, &bd, &cmat, d)?;
    Ok(normalize_pair(tc.num, tc.den))
}

/// Matrix exponential by scaling-and-squaring with a **20-term Taylor series**.
///
/// Port of `PolynomialHelpers.expm`. See the module docs: this is deliberately
/// *not* [`crate::linalg::expm`], the [6/6] Padé variant the Java uses
/// elsewhere.
pub fn expm(matrix: &Mat) -> Mat {
    let n = matrix.len();
    let mut norm = 0.0f64;
    for row in matrix {
        for v in row {
            norm = norm.max(v.abs());
        }
    }
    let s = 0.max(libm::ceil(libm::log(norm.max(1e-12)) / libm::log(2.0)) as i64 + 1);
    let sc = libm::pow(2.0, s as f64);
    let a_s: Mat = matrix
        .iter()
        .map(|row| row.iter().map(|v| v / sc).collect())
        .collect();

    let mut result = identity(n);
    let mut term = identity(n);
    for k in 1..=20 {
        term = mat_mul(&term, &a_s);
        let inv_k = 1.0 / k as f64;
        for row in &mut term {
            for v in row.iter_mut() {
                *v *= inv_k;
            }
        }
        for i in 0..n {
            for j in 0..n {
                result[i][j] += term[i][j];
            }
        }
    }
    for _ in 0..s {
        result = mat_mul(&result, &result);
    }
    result
}

fn identity(n: usize) -> Mat {
    let mut m = vec![vec![0.0; n]; n];
    for (i, row) in m.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    m
}

fn mat_mul(a: &Mat, b: &Mat) -> Mat {
    let rows = a.len();
    if rows == 0 || b.is_empty() {
        return Vec::new();
    }
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

// ---------------------------------------------------------------------------
// Partial fractions
// ---------------------------------------------------------------------------

/// Partial-fraction residues, poles, orders and direct term. Port of the
/// `ResidueResult` record.
#[derive(Debug, Clone, PartialEq)]
pub struct ResidueResult {
    pub residues: Vec<Complex>,
    pub poles: Vec<Complex>,
    pub orders: Vec<usize>,
    pub k: f64,
}

/// Heaviside partial-fraction expansion of `num/den`, the numeric
/// inverse-Laplace workflow: `num/den = Σ rᵢ/(s - pᵢ)^{orderᵢ} + k`.
///
/// Port of `residue`. Repeated poles are clustered with a **`1e-6` absolute**
/// radius and expanded through Taylor coefficients of the deflated denominator.
/// That tolerance is part of the observable behaviour: a triple root whose
/// numerically computed copies scatter further than `1e-6` is reported as three
/// distinct simple poles with enormous cancelling residues, and the Java does
/// exactly the same (verified on `1/(s+1)³`, where the oracle returns residues
/// of ±6e9).
pub fn residue(num: &[f64], den: &[f64]) -> Result<ResidueResult> {
    let b = trim_leading_zeros(num);
    let a = trim_leading_zeros(den);
    let deg_den = a.len() - 1;
    if deg_den < 1 {
        return Err(err("residue: denominator must have degree >= 1"));
    }
    if b.len() - 1 > deg_den {
        return Err(err(
            "residue: improper transfer function (numerator degree > denominator degree)",
        ));
    }
    // Split off a constant direct term when bi-proper (deg num == deg den).
    let mut k = 0.0;
    let mut b_reduced = b.clone();
    if b.len() - 1 == deg_den {
        k = b[0] / a[0];
        let diff: Vec<f64> = (0..a.len()).map(|i| b[i] - k * a[i]).collect();
        b_reduced = trim_leading_zeros(&diff);
    }
    let root_list = roots(&a)?;
    if root_list.len() != deg_den {
        return Err(err("residue: could not resolve all poles"));
    }
    let groups = cluster_poles(&root_list);
    let num_c = to_complex_descending(&b_reduced);
    let den_c = to_complex_descending(&a);

    let mut residues = vec![Complex::ZERO; deg_den];
    let mut poles = vec![Complex::ZERO; deg_den];
    let mut orders = vec![0usize; deg_den];
    let mut out = 0usize;
    for (p, m) in groups {
        // qRest(s) = den(s) / (s - p)^m, by m exact synthetic divisions.
        let mut q_rest = den_c.clone();
        for _ in 0..m {
            q_rest = deflate(&q_rest, p);
        }
        // G(s) = num(s)/qRest(s) = (s - p)^m F(s); its Taylor coefficients g_j
        // about p give A_{m-j} = g_j.
        let n_tay = taylor_coeffs(&num_c, p, m);
        let q_tay = taylor_coeffs(&q_rest, p, m);
        let g = series_divide(&n_tay, &q_tay, m);
        for j in 0..m {
            residues[out] = g[j];
            poles[out] = p;
            orders[out] = m - j;
            out += 1;
        }
    }
    Ok(ResidueResult {
        residues,
        poles,
        orders,
        k,
    })
}

/// Groups complex roots into distinct poles with their multiplicity, keeping
/// first-appearance order. Port of `clusterPoles`.
fn cluster_poles(roots: &[Complex]) -> Vec<(Complex, usize)> {
    let mut groups = Vec::new();
    let mut used = vec![false; roots.len()];
    for i in 0..roots.len() {
        if used[i] {
            continue;
        }
        let mut sum_r = roots[i].re;
        let mut sum_i = roots[i].im;
        let mut count = 1usize;
        for j in (i + 1)..roots.len() {
            if !used[j] && libm::hypot(roots[i].re - roots[j].re, roots[i].im - roots[j].im) < 1e-6
            {
                used[j] = true;
                sum_r += roots[j].re;
                sum_i += roots[j].im;
                count += 1;
            }
        }
        groups.push((
            Complex::new(sum_r / count as f64, sum_i / count as f64),
            count,
        ));
    }
    groups
}

fn to_complex_descending(coeffs: &[f64]) -> Vec<Complex> {
    coeffs.iter().map(|c| Complex::new(*c, 0.0)).collect()
}

/// Divides a descending complex polynomial by `(s - p)`, dropping the
/// remainder. Port of `deflate`; a constant input deflates to the empty
/// polynomial rather than indexing out of bounds as the Java would.
fn deflate(descending: &[Complex], p: Complex) -> Vec<Complex> {
    let n = descending.len();
    if n <= 1 {
        return Vec::new();
    }
    let mut q = vec![Complex::ZERO; n - 1];
    q[0] = descending[0];
    for i in 1..(n - 1) {
        q[i] = descending[i].add(p.multiply(q[i - 1]));
    }
    q
}

/// First `count` Taylor coefficients (ascending) of a polynomial about `p`.
/// Port of `taylorCoeffs`.
fn taylor_coeffs(descending: &[Complex], p: Complex, count: usize) -> Vec<Complex> {
    let mut work = descending.to_vec();
    let mut tay = vec![Complex::ZERO; count];
    for j in 0..count {
        if work.is_empty() {
            tay[j] = Complex::ZERO;
            continue;
        }
        // Synthetic division by (s - p): the last accumulated value is the
        // remainder, i.e. the next Taylor coefficient.
        let mut q = vec![Complex::ZERO; work.len() - 1];
        let mut acc = work[0];
        for i in 1..work.len() {
            q[i - 1] = acc;
            acc = work[i].add(p.multiply(acc));
        }
        tay[j] = acc;
        work = q;
    }
    tay
}

/// Power-series quotient `nTay/qTay` (ascending) to `count` terms.
/// Port of `seriesDivide`.
fn series_divide(n_tay: &[Complex], q_tay: &[Complex], count: usize) -> Vec<Complex> {
    let mut g = vec![Complex::ZERO; count];
    for j in 0..count {
        let mut acc = n_tay.get(j).copied().unwrap_or(Complex::ZERO);
        for i in 1..=j {
            acc = acc.subtract(q_tay[i].multiply(g[j - i]));
        }
        g[j] = acc.divide(q_tay[0]);
    }
    g
}

// ---------------------------------------------------------------------------
// Steady-state error constants
// ---------------------------------------------------------------------------

/// Static error constants `[Kp, Kv, Ka]` of an open-loop `G(s) = num/den` given
/// in lowest terms. Port of `errorConstants`.
///
/// The Java indexes `aBar[aBar.length - 1]` unguarded; a denominator that is
/// entirely zero leaves `aBar` empty and throws `ArrayIndexOutOfBounds` there.
/// This port refuses that input with an explicit message instead.
pub fn error_constants(num: &[f64], den: &[f64]) -> Result<[f64; 3]> {
    let b = trim_leading_zeros(num);
    let a = trim_leading_zeros(den);
    let mut system_type = 0usize;
    for i in (0..a.len()).rev() {
        if a[i].abs() < 1e-12 {
            system_type += 1;
        } else {
            break;
        }
    }
    if system_type >= a.len() {
        return Err(err("errorConstants: denominator cannot be zero"));
    }
    let a_bar = &a[..a.len() - system_type];
    let g0 = b[b.len() - 1] / a_bar[a_bar.len() - 1]; // lim s^type G(s)

    let mut kp = f64::INFINITY;
    let mut kv = f64::INFINITY;
    let mut ka = f64::INFINITY;
    if system_type == 0 {
        kp = g0;
        kv = 0.0;
        ka = 0.0;
    } else if system_type == 1 {
        kv = g0;
        ka = 0.0;
    } else if system_type == 2 {
        ka = g0;
    }
    Ok([kp, kv, ka])
}

// ---------------------------------------------------------------------------
// Mason's gain formula
// ---------------------------------------------------------------------------

/// Node ceiling, from the Java: the loop bitmask is a `long`.
const MASON_MAX_NODES: usize = 62;

/// Ceiling on enumerated forward paths, loops, and graph-determinant terms.
///
/// **A deliberate divergence from the Java**, in the same spirit as
/// `ode::problem::MAX_OUTPUT_SAMPLES`. `findForwardPaths` / `dfsLoop` /
/// `deltaRec` enumerate *all* simple paths, *all* simple cycles and *all*
/// non-touching cycle families; on a dense 62-node graph those are
/// astronomically many. On the JVM that is a long hang and then an
/// `OutOfMemoryError`; under `panic = "abort"` on wasm32 the allocation failure
/// **kills the worker**, which no `Result` can catch. Every signal-flow graph a
/// human writes is orders of magnitude below this bound.
const MASON_MAX_TERMS: u64 = 1_000_000;

/// A forward path or loop: accumulated gain plus the bitmask of its nodes.
#[derive(Clone, Copy)]
struct PathTerm {
    gain: f64,
    mask: u64,
}

/// Overall transmittance of a scalar signal-flow graph by Mason's gain formula.
/// `g[i][j]` is the branch gain from node `i` to node `j` (`0` means no
/// branch); `source` and `sink` are 0-based node indices. Port of `mason`.
pub fn mason(g: &Mat, source: usize, sink: usize) -> Result<f64> {
    let n = g.len();
    if n > MASON_MAX_NODES {
        return Err(err("mason: too many nodes (max 62)"));
    }
    if g.iter().any(|row| row.len() != n) {
        return Err(err("mason: gain matrix must be square"));
    }
    if source >= n || sink >= n {
        return Err(err("mason: source and sink must be node indices"));
    }
    let mut budget = MASON_MAX_TERMS;
    let mut paths = Vec::new();
    find_forward_paths(
        g,
        source,
        sink,
        1u64 << source,
        1.0,
        &mut paths,
        &mut budget,
    )?;
    let mut loops = Vec::new();
    for s in 0..n {
        dfs_loop(g, s, s, 1u64 << s, 1.0, &mut loops, &mut budget)?;
    }
    let delta = graph_determinant(&loops, 0, &mut budget)?;
    if delta.abs() < 1e-15 {
        return Err(err(
            "mason: graph determinant is zero (singular signal-flow graph)",
        ));
    }
    let mut numerator = 0.0;
    for p in &paths {
        numerator += p.gain * graph_determinant(&loops, p.mask, &mut budget)?;
    }
    Ok(numerator / delta)
}

fn spend(budget: &mut u64) -> Result<()> {
    if *budget == 0 {
        return Err(err(
            "mason: signal-flow graph has too many paths, loops or non-touching \
             loop families to enumerate",
        ));
    }
    *budget -= 1;
    Ok(())
}

fn find_forward_paths(
    g: &Mat,
    current: usize,
    sink: usize,
    mask: u64,
    gain: f64,
    out: &mut Vec<PathTerm>,
    budget: &mut u64,
) -> Result<()> {
    spend(budget)?;
    if current == sink {
        out.push(PathTerm { gain, mask });
        return Ok(());
    }
    for next in 0..g.len() {
        if g[current][next] != 0.0 && (mask & (1u64 << next)) == 0 {
            find_forward_paths(
                g,
                next,
                sink,
                mask | (1u64 << next),
                gain * g[current][next],
                out,
                budget,
            )?;
        }
    }
    Ok(())
}

fn dfs_loop(
    g: &Mat,
    start: usize,
    current: usize,
    mask: u64,
    gain: f64,
    out: &mut Vec<PathTerm>,
    budget: &mut u64,
) -> Result<()> {
    spend(budget)?;
    for next in 0..g.len() {
        let branch = g[current][next];
        if branch == 0.0 {
            continue;
        }
        if next == start {
            out.push(PathTerm {
                gain: gain * branch,
                mask,
            });
        } else if next > start && (mask & (1u64 << next)) == 0 {
            dfs_loop(
                g,
                start,
                next,
                mask | (1u64 << next),
                gain * branch,
                out,
                budget,
            )?;
        }
    }
    Ok(())
}

/// Mason's graph determinant over the loops that do not touch `exclude_mask`:
/// `1 - Σ L + Σ (non-touching pairs) - …`.
fn graph_determinant(loops: &[PathTerm], exclude_mask: u64, budget: &mut u64) -> Result<f64> {
    let avail: Vec<PathTerm> = loops
        .iter()
        .filter(|l| (l.mask & exclude_mask) == 0)
        .copied()
        .collect();
    delta_rec(&avail, 0, 0, 1.0, budget)
}

fn delta_rec(
    avail: &[PathTerm],
    pos: usize,
    used: u64,
    signed_product: f64,
    budget: &mut u64,
) -> Result<f64> {
    spend(budget)?;
    let mut total = signed_product;
    for k in pos..avail.len() {
        let l = avail[k];
        if (l.mask & used) == 0 {
            total += delta_rec(
                avail,
                k + 1,
                used | l.mask,
                signed_product * (-l.gain),
                budget,
            )?;
        }
    }
    Ok(total)
}

// ---------------------------------------------------------------------------
// TransferFunction.java — `tf(num, den)` as an `Expr`
// ---------------------------------------------------------------------------

/// Rewrites every `tf(num, den)` call in an expression into the corresponding
/// `num(variable)/den(variable)` fraction, so a transfer function written as
/// `tf([1,3],[1,3,2])` can be manipulated by the CAS. Port of
/// `TransferFunction.expandCalls`.
pub fn expand_calls(e: &Expr, variable: &str) -> Result<Expr> {
    Ok(match e {
        Expr::Call { function, args } => {
            if function == "tf" {
                return expand_tf_call(args, variable);
            }
            Expr::Call {
                function: function.clone(),
                args: map_args(args, variable)?,
            }
        }
        Expr::BinOp { op, left, right } => Expr::BinOp {
            op: *op,
            left: Box::new(expand_calls(left, variable)?),
            right: Box::new(expand_calls(right, variable)?),
        },
        Expr::Neg(operand) => Expr::Neg(Box::new(expand_calls(operand, variable)?)),
        Expr::Range { start, end } => Expr::Range {
            start: Box::new(expand_calls(start, variable)?),
            end: Box::new(expand_calls(end, variable)?),
        },
        Expr::ArrayLiteral(elements) => Expr::ArrayLiteral(map_args(elements, variable)?),
        Expr::Compare { op, left, right } => Expr::Compare {
            op: *op,
            left: Box::new(expand_calls(left, variable)?),
            right: Box::new(expand_calls(right, variable)?),
        },
        Expr::Logical { op, left, right } => Expr::Logical {
            op: *op,
            left: Box::new(expand_calls(left, variable)?),
            right: Box::new(expand_calls(right, variable)?),
        },
        Expr::Not(operand) => Expr::Not(Box::new(expand_calls(operand, variable)?)),
        Expr::ArrayAccess { name, indices } => Expr::ArrayAccess {
            name: name.clone(),
            indices: map_args(indices, variable)?,
        },
        Expr::Num { .. } | Expr::Var(_) | Expr::Str(_) => e.clone(),
    })
}

fn map_args(args: &[Expr], variable: &str) -> Result<Vec<Expr>> {
    args.iter().map(|a| expand_calls(a, variable)).collect()
}

fn expand_tf_call(args: &[Expr], variable: &str) -> Result<Expr> {
    if args.len() != 2 {
        return Err(err("tf expects two arguments: tf(num, den)"));
    }
    let num = coefficients(&args[0], "num")?;
    let den = coefficients(&args[1], "den")?;
    fraction(&num, &den, variable)
}

/// Evaluates an array-literal argument to constant coefficients.
fn coefficients(arg: &Expr, which: &str) -> Result<Vec<f64>> {
    let Expr::ArrayLiteral(rows) = arg else {
        return Err(err(format!(
            "tf {which} must be a constant array literal, e.g. [1, 3, 2]"
        )));
    };
    // A bracket literal is built as rows of cells (ArrayLiteral of
    // ArrayLiterals). A coefficient vector is 1-D, so flatten row- or
    // column-vector nesting.
    let mut elements: Vec<&Expr> = Vec::new();
    for row in rows {
        match row {
            Expr::ArrayLiteral(cells) => elements.extend(cells.iter()),
            other => elements.push(other),
        }
    }
    let scope = Scope::new();
    elements
        .into_iter()
        .map(|e| {
            eval(e, &scope)
                .map_err(|ex| err(format!("tf {which} coefficients must be constants: {ex}")))
        })
        .collect()
}

/// Builds `num/den` as a single rational expression in `variable`.
/// Port of `TransferFunction.fraction`.
pub fn fraction(num: &[f64], den: &[f64], variable: &str) -> Result<Expr> {
    if den.is_empty() {
        return Err(err("denominator must have at least one coefficient"));
    }
    Ok(Expr::BinOp {
        op: BinOp::Div,
        left: Box::new(polynomial(num, variable)),
        right: Box::new(polynomial(den, variable)),
    })
}

/// Builds a polynomial in `variable` from descending-power coefficients.
/// Port of `TransferFunction.polynomial`.
///
/// Exactly-zero coefficients are skipped and negatives become subtractions, so
/// the rendered form reads `s^2 + 3*s - 2` rather than `... + -2`. The `== 0.0`
/// and `== 1.0` tests are the Java's and stay exact: a coefficient of `1e-300`
/// is still a term, and only a literal `1` loses its multiplier.
pub fn polynomial(coeffs_descending: &[f64], variable: &str) -> Expr {
    let n = coeffs_descending.len();
    let mut poly: Option<Expr> = None;
    for i in 0..n {
        let c = coeffs_descending[i];
        if c == 0.0 {
            continue;
        }
        poly = Some(add_term(poly, c, n - 1 - i, variable));
    }
    poly.unwrap_or_else(|| Expr::num(0.0))
}

fn add_term(poly: Option<Expr>, coeff: f64, power: usize, variable: &str) -> Expr {
    // Subtraction for negative coefficients so the rendered polynomial reads
    // "s^2 + 3*s - 2" rather than "... + -2".
    let negative = coeff < 0.0;
    let t = term(coeff.abs(), power, variable);
    match poly {
        None => {
            if negative {
                Expr::Neg(Box::new(t))
            } else {
                t
            }
        }
        Some(p) => Expr::BinOp {
            op: if negative { BinOp::Sub } else { BinOp::Add },
            left: Box::new(p),
            right: Box::new(t),
        },
    }
}

/// A single `coeff * variable^power` term, with the usual `1`/`var^1`
/// simplifications.
fn term(magnitude: f64, power: usize, variable: &str) -> Expr {
    match power_expr(variable, power) {
        // power == 0: a bare constant.
        None => Expr::num(magnitude),
        Some(pe) if magnitude == 1.0 => pe,
        Some(pe) => Expr::BinOp {
            op: BinOp::Mul,
            left: Box::new(Expr::num(magnitude)),
            right: Box::new(pe),
        },
    }
}

/// `variable^power`, or `None` for power 0, or the bare variable for power 1.
fn power_expr(variable: &str, power: usize) -> Option<Expr> {
    if power == 0 {
        return None;
    }
    let var = Expr::var(variable);
    if power == 1 {
        return Some(var);
    }
    Some(Expr::BinOp {
        op: BinOp::Pow,
        left: Box::new(var),
        right: Box::new(Expr::num(power as f64)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every expected value below was produced by running the **real Java
    /// engine** (`tools/golden-dumper/classpath.sh` + a direct
    /// `PolynomialHelpers` driver), not by hand or by another Rust run.
    fn close(actual: f64, expected: f64, tol: f64, what: &str) {
        assert!(
            (actual - expected).abs() <= tol,
            "{what}: got {actual}, want {expected} (tol {tol})"
        );
    }

    fn close_slice(actual: &[f64], expected: &[f64], tol: f64, what: &str) {
        assert_eq!(actual.len(), expected.len(), "{what}: length");
        for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
            close(*a, *e, tol, &format!("{what}[{i}]"));
        }
    }

    fn close_roots(actual: &[Complex], expected: &[(f64, f64)], tol: f64, what: &str) {
        assert_eq!(actual.len(), expected.len(), "{what}: root count");
        for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
            close(a.re, e.0, tol, &format!("{what}[{i}].re"));
            close(a.im, e.1, tol, &format!("{what}[{i}].im"));
        }
    }

    // ── Polynomial arithmetic (the Java `PolynomialHelpersTest` cases) ──────

    #[test]
    fn adds_and_multiplies_polynomials() {
        close_slice(
            &add(&[1.0, 2.0, 3.0], &[4.0, 5.0]),
            &[1.0, 6.0, 8.0],
            1e-15,
            "add",
        );
        close_slice(
            &multiply(&[1.0, -2.0], &[1.0, -3.0]),
            &[1.0, -5.0, 6.0],
            1e-15,
            "multiply",
        );
    }

    #[test]
    fn raw_variants_do_not_trim_leading_zeros() {
        close_slice(
            &multiply_raw(&[0.0, 1.0, 2.0], &[0.0, 1.0, 3.0]),
            &[0.0, 0.0, 1.0, 5.0, 6.0],
            1e-15,
            "multiplyRaw",
        );
        close_slice(
            &add_raw(&[0.0, 1.0, 2.0], &[0.0, 4.0, 5.0]),
            &[0.0, 5.0, 7.0],
            1e-15,
            "addRaw",
        );
    }

    #[test]
    fn trim_collapses_an_all_zero_polynomial_to_a_single_zero() {
        assert_eq!(trim_leading_zeros(&[]), vec![0.0]);
        assert_eq!(trim_leading_zeros(&[0.0, 0.0]), vec![0.0]);
        // The threshold is absolute at 1e-15, exactly as the Java's is.
        assert_eq!(trim_leading_zeros(&[1e-16, 2.0]), vec![2.0]);
        assert_eq!(trim_leading_zeros(&[1e-14, 2.0]), vec![1e-14, 2.0]);
    }

    #[test]
    fn multiply_of_an_empty_operand_stays_empty() {
        // The Java returns `new double[0]` here rather than [0.0]; callers of
        // `series`/`parallel` rely on the width bookkeeping.
        assert!(multiply(&[], &[1.0]).is_empty());
        assert!(multiply_raw(&[1.0], &[]).is_empty());
    }

    #[test]
    fn series_parallel_and_feedback_match_the_oracle() {
        let (n, d) = series(&[0.0, 1.0], &[1.0, 1.0], &[0.0, 2.0], &[1.0, 3.0]);
        close_slice(&n, &[0.0, 0.0, 2.0], 1e-15, "series num");
        close_slice(&d, &[1.0, 4.0, 3.0], 1e-15, "series den");

        let (n, d) = parallel(&[0.0, 1.0], &[1.0, 1.0], &[0.0, 2.0], &[1.0, 3.0]);
        close_slice(&n, &[0.0, 3.0, 5.0], 1e-15, "parallel num");
        close_slice(&d, &[1.0, 4.0, 3.0], 1e-15, "parallel den");

        let (n, d) = feedback(&[0.0, 1.0], &[1.0, 1.0], &[1.0], &[1.0], 1.0);
        close_slice(&n, &[0.0, 1.0], 1e-15, "feedback num");
        close_slice(&d, &[1.0, 2.0], 1e-15, "feedback den");

        // sign = -1 is positive feedback: the loop term is subtracted.
        let (_, d) = feedback(&[0.0, 1.0], &[1.0, 1.0], &[1.0], &[1.0], -1.0);
        close_slice(&d, &[1.0, 0.0], 1e-15, "feedback(+) den");
    }

    // ── roots: values AND order ─────────────────────────────────────────────

    #[test]
    fn roots_of_a_degree_zero_polynomial_are_empty() {
        for c in [vec![], vec![0.0], vec![5.0]] {
            assert!(roots(&c).unwrap().is_empty(), "{c:?}");
        }
    }

    #[test]
    fn roots_of_a_linear_polynomial_are_solved_in_closed_form() {
        close_roots(
            &roots(&[2.0, 4.0]).unwrap(),
            &[(-2.0, 0.0)],
            0.0,
            "roots [2,4]",
        );
    }

    #[test]
    fn roots_come_back_in_the_commons_math_schur_order() {
        // Oracle: PolynomialHelpers.roots on the real Java engine. The ORDER is
        // the point — every one of these is observable as a `pole(...)` array.
        close_roots(
            &roots(&[1.0, 3.0, 2.0]).unwrap(),
            &[(-2.0, 0.0), (-1.0, 0.0)],
            1e-14,
            "s^2+3s+2",
        );
        close_roots(
            &roots(&[1.0, 6.0, 11.0, 6.0]).unwrap(),
            &[
                (-2.999_999_999_999_994_7, 0.0),
                (-2.000_000_000_000_002, 0.0),
                (-0.999_999_999_999_999_6, 0.0),
            ],
            1e-13,
            "(s+1)(s+2)(s+3)",
        );
        close_roots(
            &roots(&[1.0, 10.0, 35.0, 50.0, 24.0]).unwrap(),
            &[(-4.0, 0.0), (-3.0, 0.0), (-2.0, 0.0), (-1.0, 0.0)],
            1e-13,
            "(s+1)…(s+4)",
        );
        // A complex pair precedes the remaining real root, +i member first.
        close_roots(
            &roots(&[1.0, 1.0, 2.0, 8.0]).unwrap(),
            &[
                (0.499_999_999_999_999_1, 1.936_491_673_103_710_3),
                (0.499_999_999_999_999_1, -1.936_491_673_103_710_3),
                (-2.0, 0.0),
            ],
            1e-13,
            "s^3+s^2+2s+8",
        );
        close_roots(
            &roots(&[1.0, 2.0, 3.0, 4.0, 5.0]).unwrap(),
            &[
                (-1.287_815_479_557_649_7, 0.857_896_758_328_488_7),
                (-1.287_815_479_557_649_7, -0.857_896_758_328_488_7),
                (0.287_815_479_557_648_63, 1.416_093_080_171_907_2),
                (0.287_815_479_557_648_63, -1.416_093_080_171_907_2),
            ],
            1e-13,
            "s^4+2s^3+3s^2+4s+5",
        );
        // Degree 7 — the full interleaving of pairs and singletons.
        close_roots(
            &roots(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]).unwrap(),
            &[
                (0.810_669_773_242_469_5, 0.983_602_814_609_358_6),
                (0.810_669_773_242_469_5, -0.983_602_814_609_358_6),
                (-1.399_506_228_963_355, 0.0),
                (-1.016_359_865_480_484_2, 0.945_541_778_973_498_3),
                (-1.016_359_865_480_484_2, -0.945_541_778_973_498_3),
                (-0.094_556_793_280_310_19, 1.347_923_934_918_339_8),
                (-0.094_556_793_280_310_19, -1.347_923_934_918_339_8),
            ],
            1e-13,
            "degree 7",
        );
        // Leading zeros are trimmed before the companion matrix is built.
        close_roots(
            &roots(&[0.0, 0.0, 1.0, 3.0, 2.0]).unwrap(),
            &[(-2.0, 0.0), (-1.0, 0.0)],
            1e-14,
            "padded s^2+3s+2",
        );
    }

    #[test]
    fn roots_of_a_symmetric_companion_are_sorted_decreasing() {
        // s^2 + s - 1 has companion [[-1, 1], [1, 0]], which `isSymmetric`
        // accepts — so Commons Math takes the tridiagonal path and SORTS.
        // Oracle: [0.6180339887498947, -1.6180339887498947]. The general Schur
        // path would return them the other way round, which is why this test
        // exists at all.
        close_roots(
            &roots(&[1.0, 1.0, -1.0]).unwrap(),
            &[
                (0.618_033_988_749_894_7, 0.0),
                (-1.618_033_988_749_894_7, 0.0),
            ],
            1e-14,
            "s^2+s-1",
        );
        close_roots(
            &roots(&[1.0, 0.0, -1.0]).unwrap(),
            &[(1.0, 0.0), (-1.0, 0.0)],
            1e-14,
            "s^2-1",
        );
    }

    #[test]
    fn pole_ss_reproduces_the_symmetric_decreasing_sort() {
        // The clinching case: the diagonal order is (-2, -1) but the Java
        // returns (-1, -2). Only the symmetric branch's sort explains that.
        close_roots(
            &pole_ss(&vec![vec![-2.0, 0.0], vec![0.0, -1.0]]).unwrap(),
            &[(-1.0, 0.0), (-2.0, 0.0)],
            1e-14,
            "diag(-2,-1)",
        );
        close_roots(
            &pole_ss(&vec![
                vec![1.0, 0.0, 0.0],
                vec![0.0, 5.0, 0.0],
                vec![0.0, 0.0, 3.0],
            ])
            .unwrap(),
            &[(5.0, 0.0), (3.0, 0.0), (1.0, 0.0)],
            1e-14,
            "diag(1,5,3)",
        );
        close_roots(
            &pole_ss(&vec![
                vec![4.0, 1.0, 0.0],
                vec![1.0, 3.0, 1.0],
                vec![0.0, 1.0, 2.0],
            ])
            .unwrap(),
            &[
                (4.732_050_807_568_878_5, 0.0),
                (3.0, 0.0),
                (1.267_949_192_431_122_8, 0.0),
            ],
            1e-13,
            "symmetric tridiagonal",
        );
    }

    #[test]
    fn pole_ss_keeps_the_schur_diagonal_order_when_asymmetric() {
        // Upper-triangular: already in Schur form, so the diagonal order stands
        // — and it is NOT sorted.
        close_roots(
            &pole_ss(&vec![
                vec![-3.0, 1.0, 0.0],
                vec![0.0, -1.0, 1.0],
                vec![0.0, 0.0, -2.0],
            ])
            .unwrap(),
            &[(-3.0, 0.0), (-1.0, 0.0), (-2.0, 0.0)],
            1e-14,
            "upper triangular",
        );
        close_roots(
            &pole_ss(&vec![
                vec![1.0, 2.0, 3.0],
                vec![4.0, 5.0, 6.0],
                vec![7.0, 8.0, 10.0],
            ])
            .unwrap(),
            &[
                (16.707_493_316_124_74, 0.0),
                (-0.905_740_179_521_761_1, 0.0),
                (0.198_246_863_397_009_45, 0.0),
            ],
            1e-12,
            "dense 3x3",
        );
    }

    #[test]
    fn pole_ss_rejects_a_non_square_matrix() {
        assert!(pole_ss(&vec![vec![1.0, 2.0]]).is_err());
        assert!(pole_ss(&Vec::new()).is_err());
    }

    // ── roots ↔ coefficients ────────────────────────────────────────────────

    #[test]
    fn expand_roots_rebuilds_the_monic_polynomial() {
        close_slice(
            &expand_roots(&[Complex::new(-2.0, 0.0), Complex::new(-1.0, 0.0)]),
            &[1.0, 3.0, 2.0],
            1e-8,
            "real roots",
        );
        close_slice(
            &expand_roots(&[Complex::new(-1.0, -2.0), Complex::new(-1.0, 2.0)]),
            &[1.0, 2.0, 5.0],
            1e-8,
            "conjugate pair",
        );
        assert_eq!(expand_roots(&[]), vec![1.0]);
        // A root at the origin leaves the Java's signed zero in place.
        let z = expand_roots(&[Complex::ZERO]);
        assert_eq!(z.len(), 2);
        assert!(z[1].is_sign_negative(), "expected -0.0, got {}", z[1]);
    }

    #[test]
    fn zp2tf_matches_the_oracle() {
        let (num, den) = zp2tf(&[-1.0], &[0.0], &[-2.0, -3.0], &[0.0, 0.0], 5.0);
        close_slice(&num, &[0.0, 5.0, 5.0], 1e-12, "zp2tf num");
        close_slice(&den, &[1.0, 5.0, 6.0], 1e-12, "zp2tf den");

        let (num, den) = zp2tf(&[], &[], &[-1.0], &[0.0], 2.0);
        close_slice(&num, &[0.0, 2.0], 1e-12, "no-zero num");
        close_slice(&den, &[1.0, 1.0], 1e-12, "no-zero den");
    }

    #[test]
    fn tf2zp_splits_gain_zeros_and_poles() {
        let z = tf2zp(&[2.0, 6.0], &[1.0, 5.0, 6.0]).unwrap();
        close(z.k, 2.0, 1e-15, "k");
        close_roots(&z.zeros, &[(-3.0, 0.0)], 1e-14, "zeros");
        close_roots(&z.poles, &[(-3.0, 0.0), (-2.0, 0.0)], 1e-14, "poles");

        // A zero numerator: no zeros, gain 0, poles still resolved.
        let z = tf2zp(&[0.0], &[1.0, 3.0, 2.0]).unwrap();
        close(z.k, 0.0, 1e-15, "k");
        assert!(z.zeros.is_empty());
        close_roots(&z.poles, &[(-2.0, 0.0), (-1.0, 0.0)], 1e-14, "poles");

        assert!(tf2zp(&[1.0], &[0.0]).is_err());
    }

    // ── Frequency response ──────────────────────────────────────────────────

    #[test]
    fn bode_matches_the_oracle_for_a_first_order_lag() {
        let (mag, phase) = bode(&[1.0], &[1.0, 1.0], &[0.1, 1.0, 10.0]);
        close_slice(
            &mag,
            &[
                -0.043_213_737_826_425_59,
                -3.010_299_956_639_811_6,
                -20.043_213_737_826_427,
            ],
            1e-12,
            "mag",
        );
        close_slice(
            &phase,
            &[-5.710_593_137_499_643, -45.0, -84.289_406_862_500_37],
            1e-12,
            "phase",
        );
    }

    #[test]
    fn bode_unwraps_phase_past_minus_180() {
        // 2/(s^3+3s^2+2s) crosses -180 degrees; the oracle's third point is
        // -252.98, which only an unwrapped phase can report.
        let (mag, phase) = bode(&[2.0], &[1.0, 3.0, 2.0, 0.0], &[0.1, 1.0, 10.0]);
        close_slice(
            &mag,
            &[
                19.945_942_449_251_376,
                -3.979_400_086_720_376,
                -54.192_947_217_534_6,
            ],
            1e-11,
            "mag",
        );
        close_slice(
            &phase,
            &[
                -98.572_998_363_611_4,
                -161.565_051_177_078,
                -252.979_474_388_480_13,
            ],
            1e-11,
            "phase",
        );
    }

    #[test]
    fn nyquist_matches_the_oracle() {
        let (re, im) = nyquist(&[1.0], &[1.0, 1.0], &[0.1, 1.0, 10.0]);
        close_slice(
            &re,
            &[0.990_099_009_900_990_1, 0.5, 0.009_900_990_099_009_901],
            1e-14,
            "re",
        );
        close_slice(
            &im,
            &[-0.099_009_900_990_099_01, -0.5, -0.099_009_900_990_099_01],
            1e-14,
            "im",
        );
    }

    #[test]
    fn unwrap_shifts_by_whole_turns() {
        close_slice(
            &unwrap(&[3.0, -3.0, 3.0]),
            &[3.0, 3.283_185_307_179_586_2, 3.0],
            1e-14,
            "unwrap",
        );
        assert!(unwrap(&[]).is_empty());
    }

    #[test]
    fn margin_matches_the_oracle_including_the_no_crossing_sentinel() {
        close_slice(
            &margin(&[2.0], &[1.0, 3.0, 2.0, 0.0]),
            &[
                9.541_990_401_003_47,
                32.611_955_540_446_22,
                0.749_380_918_271_849_5,
                1.414_213_550_433_955,
            ],
            1e-9,
            "margin 2/(s^3+3s^2+2s)",
        );
        close_slice(
            &margin(&[10.0], &[1.0, 6.0, 11.0, 6.0]),
            &[
                15.562_616_787_488_519,
                90.000_057_011_448_3,
                0.999_992_536_843_482_4,
                3.316_646_096_589_108_7,
            ],
            1e-9,
            "margin 10/((s+1)(s+2)(s+3))",
        );
        // 1/(s+1) never crosses either boundary inside 1e-5 .. 1e5: both
        // margins report the 1e9 sentinel and both frequencies stay 0.
        close_slice(
            &margin(&[1.0], &[1.0, 1.0]),
            &[1e9, 1e9, 0.0, 0.0],
            0.0,
            "margin 1/(s+1)",
        );
    }

    // ── Routh–Hurwitz ───────────────────────────────────────────────────────

    #[test]
    fn routh_counts_right_half_plane_poles() {
        assert_eq!(routh(&[1.0, 6.0, 11.0, 6.0]), 0, "stable cubic");
        assert_eq!(routh(&[1.0, 1.0, 2.0, 8.0]), 2, "classic 2-RHP example");
        assert_eq!(routh(&[1.0, 2.0, 3.0, 6.0]), 0, "row of zeros");
        assert_eq!(routh(&[1.0, 1.0]), 0);
        assert_eq!(routh(&[1.0, -1.0]), 1);
        assert_eq!(routh(&[5.0]), 0, "degree 0");
        assert_eq!(routh(&[1.0, 0.0, 1.0]), 0, "jw-axis pair");
        assert_eq!(routh(&[1.0, 2.0, 3.0, 4.0, 5.0]), 2);
        assert_eq!(routh(&[1.0, 0.0, 0.0, 0.0]), 0, "triple pole at the origin");
    }

    // ── Discretisation ──────────────────────────────────────────────────────

    #[test]
    fn c2d_tustin_maps_the_integrator() {
        let (num, den) = c2d(&[1.0], &[1.0, 0.0], 0.1, Some("tustin")).unwrap();
        close_slice(&num, &[0.05, 0.05], 1e-12, "num");
        close_slice(&den, &[1.0, -1.0], 1e-12, "den");
        // "bilinear" is an alias, and `None` defaults to Tustin.
        assert_eq!(
            c2d(&[1.0], &[1.0, 0.0], 0.1, Some("BILINEAR")).unwrap().0,
            num
        );
        assert_eq!(c2d(&[1.0], &[1.0, 0.0], 0.1, None).unwrap().0, num);
    }

    #[test]
    fn c2d_and_d2c_round_trip_through_tustin() {
        let (nz, dz) = c2d(&[2.0], &[1.0, 3.0], 0.05, Some("tustin")).unwrap();
        close_slice(&nz, &[0.046_511_627_906_976_744; 2], 1e-14, "numz");
        close_slice(&dz, &[1.0, -0.860_465_116_279_069_7], 1e-14, "denz");
        let (num, den) = d2c(&nz, &dz, 0.05, Some("tustin")).unwrap();
        close_slice(&num, &[0.0, 2.0], 1e-12, "num");
        close_slice(&den, &[1.0, 3.000_000_000_000_001_3], 1e-12, "den");
    }

    #[test]
    fn c2d_zoh_matches_the_oracle() {
        let (num, den) = c2d(&[2.0], &[1.0, 2.0], 0.1, Some("zoh")).unwrap();
        close_slice(&num, &[0.0, 0.181_269_246_922_018_12], 1e-12, "num");
        close_slice(&den, &[1.0, -0.818_730_753_077_981_8], 1e-12, "den");

        let (num, den) = c2d(&[1.0], &[1.0, 3.0, 2.0], 0.1, Some("zoh")).unwrap();
        close_slice(
            &num,
            &[0.0, 0.004_527_958_503_031_357, 0.004_097_066_280_856_862],
            1e-12,
            "num",
        );
        close_slice(
            &den,
            &[1.0, -1.723_568_171_113_941_4, 0.740_818_220_681_718],
            1e-12,
            "den",
        );
    }

    #[test]
    fn c2d_tustin_handles_a_second_order_biproper_numerator() {
        let (num, den) = c2d(&[1.0, 1.0], &[1.0, 2.0, 5.0], 0.2, Some("tustin")).unwrap();
        close_slice(&num, &[0.088, 0.016, -0.072], 1e-14, "num");
        close_slice(&den, &[1.0, -1.52, 0.68], 1e-14, "den");
    }

    #[test]
    fn c2d_and_d2c_refuse_bad_arguments() {
        assert!(c2d(&[1.0], &[1.0, 1.0], 0.0, None).is_err(), "Ts = 0");
        assert!(c2d(&[1.0], &[1.0, 1.0], -1.0, None).is_err(), "Ts < 0");
        assert!(
            c2d(&[1.0], &[1.0, 1.0], f64::NAN, None).is_err(),
            "Ts = NaN"
        );
        assert!(
            c2d(&[1.0, 1.0, 1.0], &[1.0, 1.0], 0.1, None).is_err(),
            "improper"
        );
        assert!(
            c2d(&[1.0], &[1.0, 1.0], 0.1, Some("euler")).is_err(),
            "method"
        );
        assert!(
            d2c(&[1.0], &[1.0, 1.0], 0.1, Some("zoh")).is_err(),
            "d2c zoh"
        );
        assert!(d2c(&[1.0], &[1.0, 1.0], 0.0, None).is_err(), "d2c Ts");
    }

    #[test]
    fn substitute_linear_fraction_clears_the_denominator() {
        close_slice(
            &substitute_linear_fraction(&[1.0, 3.0, 2.0], 2, &[2.0, -2.0], &[1.0, 1.0]).unwrap(),
            &[12.0, -4.0, 0.0],
            1e-12,
            "substitution",
        );
        assert!(
            substitute_linear_fraction(&[1.0, 0.0, 0.0], 1, &[1.0, 0.0], &[1.0, 0.0]).is_err(),
            "degree above the reference must be refused"
        );
        assert!(
            substitute_linear_fraction(&[1.0, 1.0], 1, &[1.0, 2.0, 3.0], &[1.0, 1.0]).is_err(),
            "a higher-degree substitution must be refused, not panic"
        );
    }

    #[test]
    fn expm_taylor_matches_the_oracle() {
        let e = expm(&vec![vec![0.0, 1.0], vec![-1.0, 0.0]]);
        close_slice(
            &e[0],
            &[0.540_302_305_868_139_8, 0.841_470_984_807_896_5],
            1e-14,
            "row 0",
        );
        close_slice(
            &e[1],
            &[-0.841_470_984_807_896_5, 0.540_302_305_868_139_8],
            1e-14,
            "row 1",
        );
        let e = expm(&vec![vec![0.0, 0.0], vec![0.0, 0.0]]);
        close_slice(&e[0], &[1.0, 0.0], 1e-15, "exp(0) row 0");
        close_slice(&e[1], &[0.0, 1.0], 1e-15, "exp(0) row 1");
        let e = expm(&vec![vec![-0.3, 0.1], vec![0.0, -0.2]]);
        close_slice(
            &e[0],
            &[0.740_818_220_681_717_8, 0.077_912_532_396_264_03],
            1e-14,
            "upper triangular",
        );
    }

    // ── Partial fractions ───────────────────────────────────────────────────

    #[test]
    fn residue_expands_simple_poles_in_oracle_order() {
        let r = residue(&[1.0, 3.0], &[1.0, 3.0, 2.0]).unwrap();
        close(r.k, 0.0, 1e-12, "k");
        assert_eq!(r.orders, vec![1, 1]);
        close_roots(&r.poles, &[(-2.0, 0.0), (-1.0, 0.0)], 1e-12, "poles");
        close_roots(&r.residues, &[(-1.0, 0.0), (2.0, 0.0)], 1e-12, "residues");
    }

    #[test]
    fn residue_splits_the_direct_term_when_biproper() {
        let r = residue(&[1.0, 3.0, 5.0], &[1.0, 3.0, 2.0]).unwrap();
        close(r.k, 1.0, 1e-12, "k");
        close_roots(&r.residues, &[(-3.0, 0.0), (3.0, 0.0)], 1e-12, "residues");
    }

    #[test]
    fn residue_handles_repeated_poles() {
        // 1/(s+1)^2: A_2 = 1, A_1 = 0, reported highest order first.
        let r = residue(&[1.0], &[1.0, 2.0, 1.0]).unwrap();
        assert_eq!(r.orders, vec![2, 1]);
        close_roots(&r.residues, &[(1.0, 0.0), (0.0, 0.0)], 1e-9, "1/(s+1)^2");
        // s/(s+1)^2: A_2 = -1, A_1 = 1.
        let r = residue(&[1.0, 0.0], &[1.0, 2.0, 1.0]).unwrap();
        assert_eq!(r.orders, vec![2, 1]);
        close_roots(&r.residues, &[(-1.0, 0.0), (1.0, 0.0)], 1e-9, "s/(s+1)^2");
        // 1/(s(s+1)^2) = 1/s - 1/(s+1) - 1/(s+1)^2, the cluster first.
        let r = residue(&[1.0], &[1.0, 2.0, 1.0, 0.0]).unwrap();
        assert_eq!(r.orders, vec![2, 1, 1]);
        close_roots(
            &r.residues,
            &[(-1.0, 0.0), (-1.0, 0.0), (1.0, 0.0)],
            1e-8,
            "1/(s(s+1)^2)",
        );
    }

    #[test]
    fn residue_handles_complex_poles() {
        let r = residue(&[1.0], &[1.0, 0.0, 1.0]).unwrap();
        close_roots(&r.poles, &[(0.0, 1.0), (0.0, -1.0)], 1e-14, "poles");
        close_roots(&r.residues, &[(0.0, -0.5), (0.0, 0.5)], 1e-14, "residues");
        let r = residue(&[1.0], &[1.0, 2.0, 5.0]).unwrap();
        close_roots(&r.poles, &[(-1.0, 2.0), (-1.0, -2.0)], 1e-14, "poles");
        close_roots(&r.residues, &[(0.0, -0.25), (0.0, 0.25)], 1e-14, "residues");
    }

    #[test]
    fn residue_of_a_pure_integrator_keeps_the_signed_zero_pole() {
        let r = residue(&[1.0], &[1.0, 0.0]).unwrap();
        assert_eq!(r.orders, vec![1]);
        close_roots(&r.residues, &[(1.0, 0.0)], 1e-14, "residues");
        close(r.poles[0].re, 0.0, 0.0, "pole");
    }

    #[test]
    fn residue_refuses_improper_and_degenerate_inputs() {
        assert!(residue(&[1.0, 1.0, 1.0], &[1.0, 1.0]).is_err(), "improper");
        assert!(residue(&[1.0], &[5.0]).is_err(), "degree-0 denominator");
    }

    // ── Error constants ─────────────────────────────────────────────────────

    #[test]
    fn error_constants_follow_the_system_type() {
        close_slice(
            &error_constants(&[5.0], &[1.0, 1.0]).unwrap(),
            &[5.0, 0.0, 0.0],
            1e-12,
            "type 0",
        );
        let k = error_constants(&[5.0], &[1.0, 1.0, 0.0]).unwrap();
        assert!(k[0].is_infinite() && k[0] > 0.0);
        close(k[1], 5.0, 1e-12, "Kv");
        close(k[2], 0.0, 1e-12, "Ka");
        let k = error_constants(&[5.0], &[1.0, 1.0, 0.0, 0.0]).unwrap();
        assert!(k[0].is_infinite() && k[1].is_infinite());
        close(k[2], 5.0, 1e-12, "Ka");
        // Type 3 and above: all three constants are infinite.
        let k = error_constants(&[1.0, 2.0], &[1.0, 1.0, 0.0, 0.0, 0.0]).unwrap();
        assert!(k.iter().all(|v| v.is_infinite()));
    }

    #[test]
    fn error_constants_refuse_an_all_zero_denominator() {
        // The Java throws ArrayIndexOutOfBounds here; this port names the fault.
        assert!(error_constants(&[1.0], &[0.0]).is_err());
    }

    // ── Mason ───────────────────────────────────────────────────────────────

    #[test]
    fn mason_solves_a_single_feedback_loop() {
        let g = vec![
            vec![0.0, 2.0, 0.0],
            vec![0.0, 0.0, 3.0],
            vec![0.0, 0.5, 0.0],
        ];
        close(mason(&g, 0, 2).unwrap(), -12.0, 1e-9, "T");
    }

    #[test]
    fn mason_handles_two_non_touching_loops() {
        let g = vec![
            vec![0.0, 1.0, 0.0, 0.0],
            vec![0.0, 0.5, 1.0, 0.0],
            vec![0.0, 0.0, 0.5, 1.0],
            vec![0.0, 0.0, 0.0, 0.0],
        ];
        close(mason(&g, 0, 3).unwrap(), 4.0, 1e-9, "T");
    }

    #[test]
    fn mason_refuses_degenerate_graphs() {
        // A unity self-loop makes the determinant 1 - 1 = 0.
        let singular = vec![vec![1.0, 1.0], vec![0.0, 0.0]];
        assert!(mason(&singular, 0, 1).is_err(), "singular determinant");
        let g = vec![vec![0.0, 1.0], vec![0.0, 0.0]];
        assert!(mason(&g, 0, 5).is_err(), "sink out of range");
        assert!(mason(&vec![vec![0.0, 1.0]], 0, 1).is_err(), "not square");
        assert!(
            mason(&vec![vec![0.0; 63]; 63], 0, 1).is_err(),
            "more than 62 nodes"
        );
    }

    // ── TransferFunction: Expr construction ─────────────────────────────────

    #[test]
    fn polynomial_renders_powers_signs_and_unit_coefficients() {
        // [1, 3, -2] -> s^2 + 3*s - 2, built as ((s^2 + 3*s) - 2).
        let e = polynomial(&[1.0, 3.0, -2.0], "s");
        let Expr::BinOp { op, left, right } = &e else {
            panic!("expected a BinOp, got {e:?}");
        };
        assert_eq!(*op, BinOp::Sub);
        assert_eq!(**right, Expr::num(2.0));
        let Expr::BinOp {
            op,
            left: ll,
            right: lr,
        } = &**left
        else {
            panic!("expected a nested BinOp");
        };
        assert_eq!(*op, BinOp::Add);
        // A unit leading coefficient loses its multiplier: s^2, not 1*s^2.
        assert_eq!(
            **ll,
            Expr::BinOp {
                op: BinOp::Pow,
                left: Box::new(Expr::var("s")),
                right: Box::new(Expr::num(2.0)),
            }
        );
        // Power 1 is the bare variable.
        assert_eq!(
            **lr,
            Expr::BinOp {
                op: BinOp::Mul,
                left: Box::new(Expr::num(3.0)),
                right: Box::new(Expr::var("s")),
            }
        );
    }

    #[test]
    fn polynomial_skips_zero_coefficients_and_degenerates_to_zero() {
        assert_eq!(polynomial(&[0.0, 0.0], "s"), Expr::num(0.0));
        assert_eq!(polynomial(&[], "s"), Expr::num(0.0));
        // s^2 + 1: the middle term disappears entirely.
        assert_eq!(
            polynomial(&[1.0, 0.0, 1.0], "s"),
            Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::BinOp {
                    op: BinOp::Pow,
                    left: Box::new(Expr::var("s")),
                    right: Box::new(Expr::num(2.0)),
                }),
                right: Box::new(Expr::num(1.0)),
            }
        );
        // A leading negative becomes a Neg, not a subtraction from nothing.
        assert!(matches!(polynomial(&[-1.0], "s"), Expr::Neg(_)));
    }

    #[test]
    fn fraction_builds_a_division_and_refuses_an_empty_denominator() {
        let e = fraction(&[1.0], &[1.0, 1.0], "s").unwrap();
        assert!(matches!(e, Expr::BinOp { op: BinOp::Div, .. }));
        assert!(fraction(&[1.0], &[], "s").is_err());
    }

    #[test]
    fn expand_calls_rewrites_tf_into_a_fraction() {
        // tf([1,3],[1,3,2]) -> (s + 3) / (s^2 + 3*s + 2)
        let call = Expr::Call {
            function: "tf".to_string(),
            args: vec![
                Expr::ArrayLiteral(vec![Expr::ArrayLiteral(vec![
                    Expr::num(1.0),
                    Expr::num(3.0),
                ])]),
                Expr::ArrayLiteral(vec![Expr::ArrayLiteral(vec![
                    Expr::num(1.0),
                    Expr::num(3.0),
                    Expr::num(2.0),
                ])]),
            ],
        };
        let expanded = expand_calls(&call, "s").unwrap();
        assert_eq!(
            expanded,
            fraction(&[1.0, 3.0], &[1.0, 3.0, 2.0], "s").unwrap()
        );
        // Nested inside arithmetic, and untouched elsewhere.
        let sum = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(call.clone()),
            right: Box::new(Expr::var("x")),
        };
        let Expr::BinOp { left, right, .. } = expand_calls(&sum, "s").unwrap() else {
            panic!("structure changed");
        };
        assert_eq!(*left, expanded);
        assert_eq!(*right, Expr::var("x"));
    }

    #[test]
    fn expand_calls_refuses_non_constant_or_mis_shaped_tf_arguments() {
        let bad_arity = Expr::Call {
            function: "tf".to_string(),
            args: vec![Expr::ArrayLiteral(vec![Expr::num(1.0)])],
        };
        assert!(expand_calls(&bad_arity, "s").is_err());
        let not_a_literal = Expr::Call {
            function: "tf".to_string(),
            args: vec![Expr::var("a"), Expr::ArrayLiteral(vec![Expr::num(1.0)])],
        };
        assert!(expand_calls(&not_a_literal, "s").is_err());
        let non_constant = Expr::Call {
            function: "tf".to_string(),
            args: vec![
                Expr::ArrayLiteral(vec![Expr::var("k")]),
                Expr::ArrayLiteral(vec![Expr::num(1.0)]),
            ],
        };
        assert!(expand_calls(&non_constant, "s").is_err());
    }

    // ── Complex ─────────────────────────────────────────────────────────────

    #[test]
    fn complex_division_underflow_yields_zero_not_nan() {
        let z = Complex::new(1.0, 1.0).divide(Complex::ZERO);
        assert_eq!(z, Complex::ZERO);
        let z = Complex::new(4.0, 2.0).divide(Complex::new(2.0, 0.0));
        assert_eq!(z, Complex::new(2.0, 1.0));
    }

    #[test]
    fn eval_poly_evaluates_by_horner() {
        // (s^2 + 3s + 2) at s = j -> (2 - 1) + 3j = 1 + 3j
        let v = eval_poly(&[1.0, 3.0, 2.0], Complex::new(0.0, 1.0));
        close(v.re, 1.0, 1e-15, "re");
        close(v.im, 3.0, 1e-15, "im");
        close(Complex::new(3.0, 4.0).magnitude(), 5.0, 1e-15, "magnitude");
    }
}
