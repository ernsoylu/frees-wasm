//! State space ↔ transfer function for SISO systems.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/cas/StateSpace.java`
//! (194 LOC).
//!
//! # Why this file replaces Symja with an algorithm rather than a string
//!
//! `ss2tf` is the one place in the control suite that *did* reach Symja. The
//! Java builds the symbolic matrix `sI - A`, asks for `Det(sI - A)` and
//! `Cancel(C · (det · Inverse(sI - A)) · B)`, adds `D · det`, then reads the
//! coefficients back off with `Coefficient(tf, s, k)`. That is:
//!
//! ```text
//! den(s) = det(sI - A)                       — the characteristic polynomial
//! num(s) = C · adj(sI - A) · B + D · det(sI - A)
//! ```
//!
//! Both are produced together, in one pass and with no matrix inverse, by the
//! **Faddeev–LeVerrier** recursion — a textbook identity, not an approximation:
//!
//! ```text
//! M0 = I,   c1 = -tr(A M0)
//! Mk = A M(k-1) + ck I,   c(k+1) = -tr(A Mk)/(k+1)
//! det(sI - A) = s^n + c1 s^(n-1) + … + cn
//! adj(sI - A) = s^(n-1) M0 + s^(n-2) M1 + … + M(n-1)
//! ```
//!
//! so `num[k] = C M(k-1) B + D·ck` and `den[k] = ck`. Verified against the
//! oracle on first-, second-, third- and fourth-order systems, integer and
//! fractional, with and without direct feedthrough.
//!
//! **No cancellation.** The Java deliberately returns the *uncancelled* pair
//! (its `Cancel` only removes the `det` it multiplied in), so a coinciding
//! pole/zero pair survives in both polynomials. The recursion does the same.
//!
//! Two Symja-specific failure modes disappear with it: `ss2tf CAS error` and
//! `ss2tf system too large to convert symbolically` (a caught `StackOverflow`
//! from Symja's recursive `Det`). This port has neither — `n` states only cost
//! `O(n⁴)` arithmetic — so those two messages have no counterpart here.
//!
//! # Numerical note
//!
//! Symja computes over exact rationals; this recursion is `f64`. For the small
//! integer systems control documents contain, the coefficients come out exact
//! or within a few ulp. Faddeev–LeVerrier is known to lose accuracy for large,
//! badly scaled `A` — but so does the symbolic route once `evalDouble()`
//! collapses the exact result, and `ss2tf` is only ever called on the handful
//! of states a SISO model has.

// Numerical kernels index parallel arrays by the same loop variable, mirroring
// the Java being transcribed.
#![allow(clippy::needless_range_loop)]

use crate::diag::{FreesError, Result};
use crate::linalg::Mat;

fn err(message: impl Into<String>) -> FreesError {
    FreesError::evaluation(message)
}

/// `num(s)/den(s)` coefficient arrays, both in descending powers and both of
/// length `n + 1` (the numerator is zero-padded at the high-order end when it
/// is of lower degree). Port of the `TransferCoefficients` record.
#[derive(Debug, Clone, PartialEq)]
pub struct TransferCoefficients {
    pub num: Vec<f64>,
    pub den: Vec<f64>,
}

/// The four matrices of a state-space model, in the Java's shapes: `a` is
/// `n×n`, `b` is `n×1`, `c` is `1×n` and `d` is `1×1`. Port of the
/// `StateSpaceMatrices` record.
#[derive(Debug, Clone, PartialEq)]
pub struct StateSpaceMatrices {
    pub a: Mat,
    pub b: Mat,
    pub c: Mat,
    pub d: Mat,
}

/// `G(s) = C (sI - A)⁻¹ B + D` as `(num, den)` coefficient arrays.
/// Port of `ss2tf`; see the module docs for the Faddeev–LeVerrier substitution.
pub fn ss2tf(a: &Mat, b: &Mat, c: &Mat, d: f64) -> Result<TransferCoefficients> {
    let n = a.len();
    if n == 0 || a.iter().any(|row| row.len() != n) {
        return Err(err("ss2tf requires a square A matrix"));
    }
    if b.len() != n || b.iter().any(|row| row.len() != 1) {
        return Err(err("ss2tf requires B to be n x 1"));
    }
    if c.len() != 1 || c[0].len() != n {
        return Err(err("ss2tf requires C to be 1 x n"));
    }

    let mut den = vec![0.0; n + 1];
    let mut num = vec![0.0; n + 1];
    den[0] = 1.0;
    num[0] = d;

    // M starts as the identity (M0).
    let mut m = vec![vec![0.0; n]; n];
    for i in 0..n {
        m[i][i] = 1.0;
    }

    for k in 1..=n {
        // Numerator contribution of this Faddeev matrix: C · M(k-1) · B.
        let mut cmb = 0.0;
        for i in 0..n {
            let mut mb = 0.0;
            for j in 0..n {
                mb += m[i][j] * b[j][0];
            }
            cmb += c[0][i] * mb;
        }
        // A · M(k-1), and its trace.
        let mut am = vec![vec![0.0; n]; n];
        for i in 0..n {
            for p in 0..n {
                let aip = a[i][p];
                for j in 0..n {
                    am[i][j] += aip * m[p][j];
                }
            }
        }
        let trace: f64 = (0..n).map(|i| am[i][i]).sum();
        let ck = -trace / k as f64;
        den[k] = ck;
        num[k] = cmb + d * ck;
        // Mk = A M(k-1) + ck I.
        for i in 0..n {
            am[i][i] += ck;
        }
        m = am;
    }
    Ok(TransferCoefficients { num, den })
}

/// Transfer function → **controllable canonical** state space.
/// Port of `tf2ss`.
///
/// `num` and `den` must have the same length `n + 1`; the caller left-pads a
/// lower-degree numerator (which is what [`super::tf::c2d`]'s ZOH branch does).
/// The `A` produced is the *first* companion form — coefficients along the top
/// row, ones on the sub-diagonal — matching the Java exactly, because `B` and
/// `C` are chosen to pair with it.
pub fn tf2ss(num: &[f64], den: &[f64]) -> Result<StateSpaceMatrices> {
    if den.is_empty() {
        return Err(err("tf2ss: denominator cannot be empty"));
    }
    let n = den.len() - 1;
    if num.len() != den.len() {
        return Err(err("tf2ss: num and den must have the same length (n+1)"));
    }
    let d0 = den[0];
    if d0.abs() < 1e-15 {
        return Err(err("tf2ss: leading denominator coefficient cannot be zero"));
    }
    let a_coeffs: Vec<f64> = den.iter().map(|v| v / d0).collect();
    let b_coeffs: Vec<f64> = num.iter().map(|v| v / d0).collect();

    let d = b_coeffs[0];
    let b_bar: Vec<f64> = (1..=n).map(|i| b_coeffs[i] - d * a_coeffs[i]).collect();

    let mut a = vec![vec![0.0; n]; n];
    let mut b_mat = vec![vec![0.0; 1]; n];
    let mut c_mat = vec![vec![0.0; n]; 1];
    if n > 0 {
        for j in 0..n {
            a[0][j] = -a_coeffs[j + 1];
        }
        for i in 1..n {
            a[i][i - 1] = 1.0;
        }
        b_mat[0][0] = 1.0;
        c_mat[0][..n].copy_from_slice(&b_bar[..n]);
    }
    Ok(StateSpaceMatrices {
        a,
        b: b_mat,
        c: c_mat,
        d: vec![vec![d]],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected values come from running the **real Java engine** (Symja
    /// loaded) through a direct `StateSpace` driver, not from a second Rust
    /// run.
    fn close_slice(actual: &[f64], expected: &[f64], tol: f64, what: &str) {
        assert_eq!(actual.len(), expected.len(), "{what}: length");
        for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
            assert!(
                (a - e).abs() <= tol,
                "{what}[{i}]: got {a}, want {e} (tol {tol})"
            );
        }
    }

    fn mat(rows: &[&[f64]]) -> Mat {
        rows.iter().map(|r| r.to_vec()).collect()
    }

    // ── ss2tf ───────────────────────────────────────────────────────────────

    #[test]
    fn ss2tf_first_order() {
        let tc = ss2tf(&mat(&[&[-1.0]]), &mat(&[&[1.0]]), &mat(&[&[1.0]]), 0.0).unwrap();
        close_slice(&tc.num, &[0.0, 1.0], 1e-9, "num");
        close_slice(&tc.den, &[1.0, 1.0], 1e-9, "den");
    }

    #[test]
    fn ss2tf_puts_direct_feedthrough_in_the_numerator() {
        // D = 1 with A = -1, B = C = 1: G = 1/(s+1) + 1 = (s+2)/(s+1).
        let tc = ss2tf(&mat(&[&[-1.0]]), &mat(&[&[1.0]]), &mat(&[&[1.0]]), 1.0).unwrap();
        close_slice(&tc.num, &[1.0, 2.0], 1e-9, "num");
        close_slice(&tc.den, &[1.0, 1.0], 1e-9, "den");
    }

    #[test]
    fn ss2tf_second_order_controllable_canonical() {
        let tc = ss2tf(
            &mat(&[&[0.0, 1.0], &[-2.0, -3.0]]),
            &mat(&[&[0.0], &[1.0]]),
            &mat(&[&[1.0, 0.0]]),
            0.0,
        )
        .unwrap();
        close_slice(&tc.num, &[0.0, 0.0, 1.0], 1e-9, "num");
        close_slice(&tc.den, &[1.0, 3.0, 2.0], 1e-9, "den");
    }

    #[test]
    fn ss2tf_second_order_general_with_feedthrough() {
        // Oracle: num = [0.5, 13.5, 43], den = [1, 5, -2].
        let tc = ss2tf(
            &mat(&[&[-1.0, 2.0], &[3.0, -4.0]]),
            &mat(&[&[1.0], &[2.0]]),
            &mat(&[&[3.0, 4.0]]),
            0.5,
        )
        .unwrap();
        close_slice(&tc.num, &[0.5, 13.5, 43.0], 1e-9, "num");
        close_slice(&tc.den, &[1.0, 5.0, -2.0], 1e-9, "den");
    }

    #[test]
    fn ss2tf_diagonal_plant_with_feedthrough() {
        // A = diag(-2, -5), B = C = ones, D = 2 -> (2s^2 + 16s + 27)/(s^2+7s+10).
        let tc = ss2tf(
            &mat(&[&[-2.0, 0.0], &[0.0, -5.0]]),
            &mat(&[&[1.0], &[1.0]]),
            &mat(&[&[1.0, 1.0]]),
            2.0,
        )
        .unwrap();
        close_slice(&tc.num, &[2.0, 16.0, 27.0], 1e-9, "num");
        close_slice(&tc.den, &[1.0, 7.0, 10.0], 1e-9, "den");
    }

    #[test]
    fn ss2tf_third_and_fourth_order() {
        let tc = ss2tf(
            &mat(&[&[0.0, 1.0, 0.0], &[0.0, 0.0, 1.0], &[-6.0, -11.0, -6.0]]),
            &mat(&[&[0.0], &[0.0], &[1.0]]),
            &mat(&[&[1.0, 0.0, 0.0]]),
            0.0,
        )
        .unwrap();
        close_slice(&tc.num, &[0.0, 0.0, 0.0, 1.0], 1e-9, "3rd num");
        close_slice(&tc.den, &[1.0, 6.0, 11.0, 6.0], 1e-9, "3rd den");

        let tc = ss2tf(
            &mat(&[
                &[0.0, 1.0, 0.0, 0.0],
                &[0.0, 0.0, 1.0, 0.0],
                &[0.0, 0.0, 0.0, 1.0],
                &[-24.0, -50.0, -35.0, -10.0],
            ]),
            &mat(&[&[0.0], &[0.0], &[0.0], &[1.0]]),
            &mat(&[&[2.0, 1.0, 0.0, 0.0]]),
            0.0,
        )
        .unwrap();
        close_slice(&tc.num, &[0.0, 0.0, 0.0, 1.0, 2.0], 1e-9, "4th num");
        close_slice(&tc.den, &[1.0, 10.0, 35.0, 50.0, 24.0], 1e-9, "4th den");
    }

    #[test]
    fn ss2tf_does_not_cancel_a_coinciding_pole_and_zero() {
        // A = diag(-1, -2), B = [1; 0], C = [1, 0]: G = (s+2)/((s+1)(s+2)).
        // The Java's `Cancel` only removes the det it multiplied in, so the
        // common (s+2) factor survives in both polynomials.
        let tc = ss2tf(
            &mat(&[&[-1.0, 0.0], &[0.0, -2.0]]),
            &mat(&[&[1.0], &[0.0]]),
            &mat(&[&[1.0, 0.0]]),
            0.0,
        )
        .unwrap();
        close_slice(&tc.num, &[0.0, 1.0, 2.0], 1e-9, "num keeps (s+2)");
        close_slice(&tc.den, &[1.0, 3.0, 2.0], 1e-9, "den");
    }

    #[test]
    fn ss2tf_validates_its_matrix_shapes() {
        let a = mat(&[&[0.0, 1.0], &[-2.0, -3.0]]);
        let b = mat(&[&[0.0], &[1.0]]);
        let c = mat(&[&[1.0, 0.0]]);
        assert!(ss2tf(&Vec::new(), &b, &c, 0.0).is_err(), "empty A");
        assert!(
            ss2tf(&mat(&[&[1.0, 2.0]]), &b, &c, 0.0).is_err(),
            "A not square"
        );
        assert!(
            ss2tf(&a, &mat(&[&[1.0]]), &c, 0.0).is_err(),
            "B wrong height"
        );
        assert!(
            ss2tf(&a, &mat(&[&[1.0, 2.0], &[3.0, 4.0]]), &c, 0.0).is_err(),
            "B wrong width"
        );
        assert!(
            ss2tf(&a, &b, &mat(&[&[1.0]]), 0.0).is_err(),
            "C wrong width"
        );
        assert!(
            ss2tf(&a, &b, &mat(&[&[1.0, 0.0], &[0.0, 1.0]]), 0.0).is_err(),
            "C wrong height"
        );
    }

    // ── tf2ss ───────────────────────────────────────────────────────────────

    #[test]
    fn tf2ss_first_order() {
        let m = tf2ss(&[0.0, 1.0], &[1.0, 1.0]).unwrap();
        assert_eq!(m.a, mat(&[&[-1.0]]));
        assert_eq!(m.b, mat(&[&[1.0]]));
        assert_eq!(m.c, mat(&[&[1.0]]));
        assert_eq!(m.d, mat(&[&[0.0]]));
    }

    #[test]
    fn tf2ss_second_order_is_the_first_companion_form() {
        let m = tf2ss(&[0.0, 1.0, 2.0], &[1.0, 3.0, 2.0]).unwrap();
        assert_eq!(m.a, mat(&[&[-3.0, -2.0], &[1.0, 0.0]]));
        assert_eq!(m.b, mat(&[&[1.0], &[0.0]]));
        assert_eq!(m.c, mat(&[&[1.0, 2.0]]));
        assert_eq!(m.d, mat(&[&[0.0]]));
    }

    #[test]
    fn tf2ss_splits_the_feedthrough_of_a_biproper_system() {
        // (2s^2+3s+4)/(s^2+3s+2): D = 2 and C = bBar = [3-6, 4-4] = [-3, 0].
        let m = tf2ss(&[2.0, 3.0, 4.0], &[1.0, 3.0, 2.0]).unwrap();
        assert_eq!(m.c, mat(&[&[-3.0, 0.0]]));
        assert_eq!(m.d, mat(&[&[2.0]]));
    }

    #[test]
    fn tf2ss_of_a_pure_gain_has_no_states() {
        // 5/2 with n = 0: A is 0x0, B is 0x1, C is 1x0, D = 2.5.
        let m = tf2ss(&[5.0], &[2.0]).unwrap();
        assert!(m.a.is_empty());
        assert!(m.b.is_empty());
        assert_eq!(m.c, vec![Vec::<f64>::new()]);
        assert_eq!(m.d, mat(&[&[2.5]]));
    }

    #[test]
    fn tf2ss_normalises_by_the_leading_denominator_coefficient() {
        // 4/(2s+6) == 2/(s+3).
        let m = tf2ss(&[0.0, 4.0], &[2.0, 6.0]).unwrap();
        assert_eq!(m.a, mat(&[&[-3.0]]));
        assert_eq!(m.c, mat(&[&[2.0]]));
    }

    #[test]
    fn tf2ss_refuses_mismatched_or_degenerate_input() {
        assert!(tf2ss(&[1.0], &[1.0, 2.0]).is_err(), "length mismatch");
        assert!(
            tf2ss(&[0.0, 1.0], &[0.0, 1.0]).is_err(),
            "zero leading coefficient"
        );
        assert!(tf2ss(&[], &[]).is_err(), "empty denominator");
    }

    // ── Round trip ──────────────────────────────────────────────────────────

    #[test]
    fn tf2ss_and_ss2tf_round_trip_a_fractional_system() {
        // No oracle for this one: Symja does not return on a 3x3 with decimal
        // entries (the run was still in `Det` after ten minutes), which is
        // itself part of why the symbolic route was replaced. Round-tripping
        // checks the pair against each other instead.
        let num = [0.0, 0.5, 1.25, 0.75];
        let den = [1.0, 2.5, 3.25, 1.5];
        let ss = tf2ss(&num, &den).unwrap();
        let back = ss2tf(&ss.a, &ss.b, &ss.c, ss.d[0][0]).unwrap();
        close_slice(&back.num, &num, 1e-12, "num");
        close_slice(&back.den, &den, 1e-12, "den");
    }

    #[test]
    fn ss2tf_of_a_transformed_realisation_is_invariant() {
        // Similarity transform x = T z with T = [[1, 1], [0, 1]] must leave the
        // transfer function alone — a property the Faddeev recursion inherits
        // from the determinant it computes.
        let a = mat(&[&[0.0, 1.0], &[-2.0, -3.0]]);
        let b = mat(&[&[0.0], &[1.0]]);
        let c = mat(&[&[1.0, 0.0]]);
        let base = ss2tf(&a, &b, &c, 0.0).unwrap();
        // T = [[1,1],[0,1]], T^-1 = [[1,-1],[0,1]]
        let at = mat(&[&[2.0, 6.0], &[-2.0, -5.0]]); // T^-1 A T
        let bt = mat(&[&[-1.0], &[1.0]]); // T^-1 B
        let ct = mat(&[&[1.0, 1.0]]); // C T
        let moved = ss2tf(&at, &bt, &ct, 0.0).unwrap();
        close_slice(&moved.num, &base.num, 1e-12, "num");
        close_slice(&moved.den, &base.den, 1e-12, "den");
    }
}
