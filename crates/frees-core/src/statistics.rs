//! Statistical regression kernels: `LinFit` / `PolyFit`.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/core/Statistics.java`
//! (48 LOC). The Java delegates to Apache Commons Math:
//!
//! * [`lin_fit`] ports `SimpleRegression` (with intercept) — the exact
//!   Welford-style update from `SimpleRegression.addData` plus the
//!   `getSlope`/`getIntercept`/`getRSquare` getters, including the
//!   `NaN → 0.0` degenerate-R² rule the Java `linFit` applies.
//! * [`poly_fit`] replaces `PolynomialCurveFitter` (a Levenberg–Marquardt
//!   optimizer that, on this *linear* model, converges to the ordinary
//!   least-squares optimum) with a direct Householder-QR least-squares solve
//!   of the Vandermonde system — the same optimum, computed directly.
//!
//! The evaluator consumes these through the synthetic `linfit$…` / `polyfit$…`
//! calls that `CALL LinFit` / `CALL PolyFit` flatten into.

// Numerical kernels index several parallel arrays (and 2-D `a[i][j]` slices)
// by the same loop variable, mirroring the Java/Fortran sources being
// transcribed. Iterator rewrites obscure that correspondence, so the indexed
// form stays.
#![allow(clippy::needless_range_loop)]

use crate::diag::{FreesError, Result};

/// Smallest positive `double` (Java `Double.MIN_VALUE`, the subnormal
/// `4.9e-324`) — **not** Rust's `f64::MIN_POSITIVE`, which is the smallest
/// *normal* value. `SimpleRegression.getSlope` compares against ten times this.
fn java_double_min() -> f64 {
    f64::from_bits(1)
}

/// `Math.max(0, v)` with Java NaN semantics (`NaN` propagates).
fn java_max0(v: f64) -> f64 {
    if v.is_nan() {
        f64::NAN
    } else {
        v.max(0.0)
    }
}

/// Ordinary least-squares line fit: `[slope, intercept, r_squared]`.
///
/// Port of `Statistics.linFit`: a `SimpleRegression(true)` fed point by point,
/// with a degenerate (`NaN`) R² reported as `0.0`. A degenerate *slope*
/// (fewer than two points, or no variation in x) stays `NaN`, as in Java.
pub fn lin_fit(x: &[f64], y: &[f64]) -> Result<[f64; 3]> {
    if x.len() != y.len() {
        return Err(FreesError::evaluation(
            "LinFit x and y must have equal length.",
        ));
    }

    // SimpleRegression.addData, hasIntercept = true.
    let mut n: u64 = 0;
    let (mut xbar, mut ybar) = (0.0_f64, 0.0_f64);
    let (mut sum_x, mut sum_y) = (0.0_f64, 0.0_f64);
    let (mut sum_xx, mut sum_yy, mut sum_xy) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (&xi, &yi) in x.iter().zip(y.iter()) {
        if n == 0 {
            xbar = xi;
            ybar = yi;
        } else {
            let fact1 = 1.0 + n as f64;
            let fact2 = n as f64 / fact1;
            let dx = xi - xbar;
            let dy = yi - ybar;
            sum_xx += dx * dx * fact2;
            sum_yy += dy * dy * fact2;
            sum_xy += dx * dy * fact2;
            xbar += dx / fact1;
            ybar += dy / fact1;
        }
        sum_x += xi;
        sum_y += yi;
        n += 1;
    }

    // getSlope
    let slope = if n < 2 || libm::fabs(sum_xx) < 10.0 * java_double_min() {
        f64::NAN
    } else {
        sum_xy / sum_xx
    };
    // getIntercept
    let intercept = (sum_y - slope * sum_x) / n as f64;
    // getRSquare = (SSTO − SSE) / SSTO, SSE = max(0, sumYY − sumXY²/sumXX)
    let ssto = if n < 2 { f64::NAN } else { sum_yy };
    let sse = java_max0(sum_yy - sum_xy * sum_xy / sum_xx);
    let mut r2 = (ssto - sse) / ssto;
    if r2.is_nan() {
        r2 = 0.0; // degenerate (e.g. all-identical x): report 0 rather than NaN
    }
    Ok([slope, intercept, r2])
}

/// Least-squares polynomial fit of the given degree. Returns the coefficients
/// in ascending power order: `c[0] + c[1]·x + … + c[degree]·x^degree`
/// (length `degree + 1`).
///
/// Port of `Statistics.polyFit`. The Java fits with `PolynomialCurveFitter`
/// (Levenberg–Marquardt); on a model that is linear in its coefficients that
/// optimizer converges to the unique least-squares optimum, which this port
/// computes directly by Householder QR on the Vandermonde matrix.
pub fn poly_fit(x: &[f64], y: &[f64], degree: usize) -> Result<Vec<f64>> {
    if x.len() != y.len() {
        return Err(FreesError::evaluation(
            "PolyFit x and y must have equal length.",
        ));
    }
    let p = degree + 1;
    if x.len() < p {
        return Err(FreesError::evaluation(format!(
            "PolyFit of degree {degree} needs at least {p} points, got {}.",
            x.len()
        )));
    }
    // Vandermonde: row i = [1, x_i, x_i², …, x_i^degree].
    let mut a: Vec<Vec<f64>> = Vec::with_capacity(x.len());
    for &xi in x {
        let mut row = Vec::with_capacity(p);
        let mut pw = 1.0;
        for _ in 0..p {
            row.push(pw);
            pw *= xi;
        }
        a.push(row);
    }
    qr_least_squares(a, y.to_vec(), p)
}

/// Minimum-residual solution of the overdetermined system `A·c = b` via
/// Householder QR. `A` is `m×p`, `m ≥ p`.
fn qr_least_squares(mut a: Vec<Vec<f64>>, mut b: Vec<f64>, p: usize) -> Result<Vec<f64>> {
    let m = a.len();
    for k in 0..p {
        // Householder reflector for column k, rows k..m.
        let mut norm_sq = 0.0;
        for row in a.iter().take(m).skip(k) {
            norm_sq += row[k] * row[k];
        }
        let norm = libm::sqrt(norm_sq);
        if norm == 0.0 {
            return Err(FreesError::evaluation(
                "PolyFit: the fit is singular (degenerate x data).",
            ));
        }
        let alpha = if a[k][k] > 0.0 { -norm } else { norm };
        let mut v: Vec<f64> = (k..m).map(|i| a[i][k]).collect();
        v[0] -= alpha;
        let v_norm_sq: f64 = v.iter().map(|t| t * t).sum();
        if v_norm_sq > 0.0 {
            // Apply I − 2vvᵀ/(vᵀv) to the remaining columns and to b.
            for j in k..p {
                let dot: f64 = (k..m).map(|i| v[i - k] * a[i][j]).sum();
                let scale = 2.0 * dot / v_norm_sq;
                for i in k..m {
                    a[i][j] -= scale * v[i - k];
                }
            }
            let dot: f64 = (k..m).map(|i| v[i - k] * b[i]).sum();
            let scale = 2.0 * dot / v_norm_sq;
            for i in k..m {
                b[i] -= scale * v[i - k];
            }
        }
        a[k][k] = alpha;
    }
    // Back-substitute R·c = Qᵀb (upper p×p triangle of A, transformed b).
    let mut c = vec![0.0; p];
    for i in (0..p).rev() {
        let mut s = b[i];
        for j in i + 1..p {
            s -= a[i][j] * c[j];
        }
        if a[i][i] == 0.0 {
            return Err(FreesError::evaluation(
                "PolyFit: the fit is singular (degenerate x data).",
            ));
        }
        c[i] = s / a[i][i];
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) {
        assert!(
            (a - b).abs() <= tol * b.abs().max(1.0),
            "expected {b}, got {a}"
        );
    }

    #[test]
    fn lin_fit_recovers_an_exact_line() {
        let x = [0.0, 1.0, 2.0, 3.0];
        let y = [1.0, 3.0, 5.0, 7.0]; // y = 2x + 1
        let [slope, intercept, r2] = lin_fit(&x, &y).unwrap();
        close(slope, 2.0, 1e-12);
        close(intercept, 1.0, 1e-12);
        close(r2, 1.0, 1e-12);
    }

    #[test]
    fn lin_fit_matches_the_textbook_values_for_a_scattered_set() {
        // x̄=2, ȳ=10/3: Sxy=3, Sxx=2 → slope 1.5, intercept 1/3,
        // Syy=14/3, SSE=1/6 → R² = 27/28.
        let x = [1.0, 2.0, 3.0];
        let y = [2.0, 3.0, 5.0];
        let [slope, intercept, r2] = lin_fit(&x, &y).unwrap();
        close(slope, 1.5, 1e-12);
        close(intercept, 1.0 / 3.0, 1e-12);
        close(r2, 27.0 / 28.0, 1e-12);
    }

    #[test]
    fn lin_fit_degenerate_x_reports_nan_slope_and_zero_r2() {
        // All-identical x: Java SimpleRegression yields NaN slope; linFit
        // maps the NaN R² to 0.0.
        let x = [2.0, 2.0, 2.0];
        let y = [1.0, 2.0, 3.0];
        let [slope, _intercept, r2] = lin_fit(&x, &y).unwrap();
        assert!(slope.is_nan());
        assert_eq!(r2, 0.0);
    }

    #[test]
    fn lin_fit_constant_y_has_zero_slope_and_zero_r2() {
        // SSTO = 0 → R² = 0/0 = NaN → 0.0 per the Java rule.
        let x = [1.0, 2.0, 3.0];
        let y = [5.0, 5.0, 5.0];
        let [slope, intercept, r2] = lin_fit(&x, &y).unwrap();
        close(slope, 0.0, 1e-12);
        close(intercept, 5.0, 1e-12);
        assert_eq!(r2, 0.0);
    }

    #[test]
    fn lin_fit_rejects_mismatched_lengths() {
        let err = lin_fit(&[1.0, 2.0], &[1.0]).unwrap_err().to_string();
        assert!(err.contains("equal length"), "{err}");
    }

    #[test]
    fn poly_fit_recovers_an_exact_quadratic() {
        // y = 1 − 2x + 0.5x²
        let x = [-2.0, -1.0, 0.0, 1.0, 2.0, 3.0];
        let y: Vec<f64> = x.iter().map(|&t| 1.0 - 2.0 * t + 0.5 * t * t).collect();
        let c = poly_fit(&x, &y, 2).unwrap();
        assert_eq!(c.len(), 3);
        close(c[0], 1.0, 1e-10);
        close(c[1], -2.0, 1e-10);
        close(c[2], 0.5, 1e-10);
    }

    #[test]
    fn poly_fit_degree_one_agrees_with_lin_fit() {
        let x = [1.0, 2.0, 3.0];
        let y = [2.0, 3.0, 5.0];
        let c = poly_fit(&x, &y, 1).unwrap();
        close(c[0], 1.0 / 3.0, 1e-12);
        close(c[1], 1.5, 1e-12);
    }

    #[test]
    fn poly_fit_overdetermined_least_squares_known_values() {
        // Fit y = c0 + c1 x to (0,0), (1,1), (2,1): least squares gives
        // c1 = Sxy/Sxx = 1/2, c0 = ȳ − c1 x̄ = 2/3 − 1/2 = 1/6.
        let c = poly_fit(&[0.0, 1.0, 2.0], &[0.0, 1.0, 1.0], 1).unwrap();
        close(c[0], 1.0 / 6.0, 1e-12);
        close(c[1], 0.5, 1e-12);
    }

    #[test]
    fn poly_fit_rejects_mismatched_and_underdetermined_input() {
        let err = poly_fit(&[1.0], &[1.0, 2.0], 1).unwrap_err().to_string();
        assert!(err.contains("equal length"), "{err}");
        let err = poly_fit(&[1.0, 2.0], &[1.0, 2.0], 2)
            .unwrap_err()
            .to_string();
        assert!(err.contains("at least 3 points"), "{err}");
    }

    #[test]
    fn poly_fit_degenerate_x_is_singular() {
        let err = poly_fit(&[1.0, 1.0, 1.0], &[1.0, 2.0, 3.0], 1)
            .unwrap_err()
            .to_string();
        assert!(err.contains("singular"), "{err}");
    }
}
