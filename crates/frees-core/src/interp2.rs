//! Regular-grid 2-D interpolation, `z = f(x, y)` — the `Interp2` kernel.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/core/Interpolation2D.java`
//! (65 LOC). Grids with at least 5 nodes per axis use the Commons Math
//! *piecewise bicubic spline*; smaller grids fall back to bilinear
//! interpolation. Queries outside the grid clamp to the boundary (no
//! extrapolation), matching the 1-D `CurveInterpolator`.
//!
//! The Commons Math 3.6.1 `PiecewiseBicubicSplineInterpolatingFunction` this
//! ports does **not** fit a tensor bicubic polynomial: for every query it
//! selects the 5×5 sub-grid around the point, runs an **Akima cubic spline**
//! along x through each of the five z-columns, and then one more Akima spline
//! along y through those five results. The Akima construction here is the
//! exact algorithm of `AkimaSplineInterpolator` (three-point end derivatives,
//! weighted-slope interior derivatives, Hermite segments).

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

use crate::diag::{FreesError, Result};

/// `Math.min` (NaN-propagating, `-0.0 < 0.0`).
fn java_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else if b < a {
        b
    } else if a.is_sign_negative() {
        a
    } else {
        b
    }
}

/// `Math.max` (NaN-propagating, `0.0 > -0.0`).
fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else if b > a {
        b
    } else if a.is_sign_positive() {
        a
    } else {
        b
    }
}

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    java_max(lo, java_min(hi, v))
}

/// Interpolates the grid `z[i][j] = f(x[i], y[j])` at `(xq, yq)`.
/// `x` (length m) and `y` (length n) must be strictly increasing.
///
/// Port of `Interpolation2D.interpolate`.
pub fn interpolate(x: &[f64], y: &[f64], z: &[Vec<f64>], xq: f64, yq: f64) -> Result<f64> {
    let m = x.len();
    let n = y.len();
    if m < 2 || n < 2 {
        return Err(FreesError::evaluation(
            "Interp2 requires at least a 2x2 grid.",
        ));
    }
    // The Java indexes z[i][j] unchecked (a shape mismatch is an
    // ArrayIndexOutOfBoundsException); this port validates with a message.
    if z.len() != m || z.iter().any(|row| row.len() != n) {
        return Err(FreesError::evaluation(format!(
            "Interp2 grid must be {m}x{n} to match x and y."
        )));
    }
    let cx = clamp(xq, x[0], x[m - 1]);
    let cy = clamp(yq, y[0], y[n - 1]);
    if m >= 5 && n >= 5 {
        return piecewise_bicubic(x, y, z, cx, cy);
    }
    Ok(bilinear(x, y, z, cx, cy))
}

/// Port of `Interpolation2D.bilinear` (including its tolerance of NaN, which
/// simply propagates).
fn bilinear(x: &[f64], y: &[f64], z: &[Vec<f64>], xq: f64, yq: f64) -> f64 {
    let i = upper_index(x, xq);
    let j = upper_index(y, yq);
    let x0 = x[i - 1];
    let x1 = x[i];
    let y0 = y[j - 1];
    let y1 = y[j];
    let tx = if x1 == x0 { 0.0 } else { (xq - x0) / (x1 - x0) };
    let ty = if y1 == y0 { 0.0 } else { (yq - y0) / (y1 - y0) };
    let z00 = z[i - 1][j - 1];
    let z10 = z[i][j - 1];
    let z01 = z[i - 1][j];
    let z11 = z[i][j];
    let zx0 = z00 + tx * (z10 - z00);
    let zx1 = z01 + tx * (z11 - z01);
    zx0 + ty * (zx1 - zx0)
}

/// First index k (>= 1) with `grid[k] >= q`, so the bracket is `[k-1, k]`.
fn upper_index(grid: &[f64], q: f64) -> usize {
    for k in 1..grid.len() {
        if q <= grid[k] {
            return k;
        }
    }
    grid.len() - 1
}

/// Port of `PiecewiseBicubicSplineInterpolatingFunction` (constructor
/// validation + `value`): pick the 5×5 window around the query, Akima along x
/// per z-column, then Akima along y through the five column results.
fn piecewise_bicubic(x: &[f64], y: &[f64], z: &[Vec<f64>], xq: f64, yq: f64) -> Result<f64> {
    check_strictly_increasing("Interp2 x", x)?;
    check_strictly_increasing("Interp2 y", y)?;
    let i = search_index(xq, x)?;
    let j = search_index(yq, y)?;

    let x_win = &x[i..i + 5];
    let y_win = &y[j..j + 5];
    let mut interp = [0.0_f64; 5];
    let mut column = [0.0_f64; 5];
    for (z_idx, slot) in interp.iter_mut().enumerate() {
        for (idx, cell) in column.iter_mut().enumerate() {
            *cell = z[i + idx][j + z_idx];
        }
        *slot = akima_value(x_win, &column, xq)?;
    }
    akima_value(y_win, &interp, yq)
}

/// `PiecewiseBicubicSplineInterpolatingFunction.searchIndex` with
/// `offset = 2, count = 5`: window start for the 5-node neighbourhood,
/// clamped to the grid.
fn search_index(c: f64, val: &[f64]) -> Result<usize> {
    let len = val.len() as isize;
    let r: isize = match val.binary_search_by(|v| v.total_cmp(&c)) {
        Ok(idx) => idx as isize - 2,
        Err(ip) => {
            if ip == 0 || ip == val.len() {
                // Below the first or above the last sample — unreachable after
                // the caller's clamp except for NaN queries, where the Java
                // throws OutOfRangeException.
                return Err(FreesError::evaluation(format!(
                    "Interp2: query {c} is outside the grid range [{}, {}]",
                    val[0],
                    val[val.len() - 1]
                )));
            }
            ip as isize - 2
        }
    };
    Ok(r.clamp(0, len - 5) as usize)
}

fn check_strictly_increasing(what: &str, v: &[f64]) -> Result<()> {
    for w in v.windows(2) {
        if !(w[1] > w[0]) {
            return Err(FreesError::evaluation(format!(
                "{what} values must be strictly increasing."
            )));
        }
    }
    Ok(())
}

/// True when `w` is within one ulp of `+0.0` — `Precision.equals(w, 0.0)`
/// for the non-negative weights the Akima scheme produces.
fn ulp1_zero(w: f64) -> bool {
    w.abs() <= f64::from_bits(1)
}

/// Build the Akima cubic spline through `(xv, yv)` and evaluate it at `q`.
/// Exact port of `AkimaSplineInterpolator.interpolate` +
/// `PolynomialSplineFunction.value` for a window of at least 5 points.
fn akima_value(xv: &[f64], yv: &[f64], q: f64) -> Result<f64> {
    debug_assert!(xv.len() >= 5 && xv.len() == yv.len());
    if q.is_nan() {
        // Java: NaN passes the range check and evaluates to NaN.
        return Ok(f64::NAN);
    }
    let n = xv.len();
    let mut differences = vec![0.0_f64; n - 1];
    let mut weights = vec![0.0_f64; n - 1];
    for i in 0..n - 1 {
        differences[i] = (yv[i + 1] - yv[i]) / (xv[i + 1] - xv[i]);
    }
    for i in 1..n - 1 {
        weights[i] = libm::fabs(differences[i] - differences[i - 1]);
    }

    let mut fd = vec![0.0_f64; n];
    for i in 2..n - 2 {
        let w_p = weights[i + 1];
        let w_m = weights[i - 1];
        if ulp1_zero(w_p) && ulp1_zero(w_m) {
            let xi = xv[i];
            let xi_p = xv[i + 1];
            let xi_m = xv[i - 1];
            fd[i] = (((xi_p - xi) * differences[i - 1]) + ((xi - xi_m) * differences[i]))
                / (xi_p - xi_m);
        } else {
            fd[i] = ((w_p * differences[i - 1]) + (w_m * differences[i])) / (w_p + w_m);
        }
    }
    fd[0] = differentiate_three_point(xv, yv, 0, 0, 1, 2);
    fd[1] = differentiate_three_point(xv, yv, 1, 0, 1, 2);
    fd[n - 2] = differentiate_three_point(xv, yv, n - 2, n - 3, n - 2, n - 1);
    fd[n - 1] = differentiate_three_point(xv, yv, n - 1, n - 3, n - 2, n - 1);

    // interpolateHermiteSorted + PolynomialSplineFunction.value.
    let i = spline_interval(xv, q);
    let w = xv[i + 1] - xv[i];
    let w2 = w * w;
    let yv0 = yv[i];
    let yv1 = yv[i + 1];
    let fd0 = fd[i];
    let fd1 = fd[i + 1];
    let c0 = yv0;
    let c1 = fd0;
    let c2 = (3.0 * (yv1 - yv0) / w - 2.0 * fd0 - fd1) / w;
    let c3 = (2.0 * (yv0 - yv1) / w + fd0 + fd1) / w2;
    let t = q - xv[i];
    Ok(c0 + t * (c1 + t * (c2 + t * c3)))
}

/// `differentiateThreePoint` from `AkimaSplineInterpolator` (Math.NET-style
/// quadratic through three samples, differentiated at the target abscissa).
fn differentiate_three_point(
    xv: &[f64],
    yv: &[f64],
    index_of_differentiation: usize,
    first: usize,
    second: usize,
    third: usize,
) -> f64 {
    let x0 = yv[first];
    let x1 = yv[second];
    let x2 = yv[third];
    let t = xv[index_of_differentiation] - xv[first];
    let t1 = xv[second] - xv[first];
    let t2 = xv[third] - xv[first];
    let a = (x2 - x0 - (t2 / t1 * (x1 - x0))) / (t2 * t2 - t1 * t2);
    let b = (x1 - x0 - a * t1 * t1) / t1;
    (2.0 * a * t) + b
}

/// `PolynomialSplineFunction.value`'s knot search: the polynomial index for a
/// query inside `[knots[0], knots[n-1]]`.
pub(crate) fn spline_interval(knots: &[f64], v: f64) -> usize {
    let i = match knots.binary_search_by(|k| k.total_cmp(&v)) {
        Ok(idx) => idx,
        Err(ip) => ip.saturating_sub(1),
    };
    if i >= knots.len() - 1 {
        knots.len() - 2
    } else {
        i
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) {
        assert!(
            (a - b).abs() <= 1e-10 * b.abs().max(1.0),
            "expected {b}, got {a}"
        );
    }

    fn grid(
        m: usize,
        n: usize,
        f: impl Fn(f64, f64) -> f64,
    ) -> (Vec<f64>, Vec<f64>, Vec<Vec<f64>>) {
        let x: Vec<f64> = (0..m).map(|i| i as f64).collect();
        let y: Vec<f64> = (0..n).map(|j| j as f64).collect();
        let z = x
            .iter()
            .map(|&xi| y.iter().map(|&yj| f(xi, yj)).collect())
            .collect();
        (x, y, z)
    }

    #[test]
    fn bilinear_2x2_center_and_corners() {
        let x = [0.0, 1.0];
        let y = [0.0, 1.0];
        let z = vec![vec![0.0, 1.0], vec![2.0, 3.0]];
        close(interpolate(&x, &y, &z, 0.5, 0.5).unwrap(), 1.5);
        close(interpolate(&x, &y, &z, 0.0, 0.0).unwrap(), 0.0);
        close(interpolate(&x, &y, &z, 1.0, 1.0).unwrap(), 3.0);
        close(interpolate(&x, &y, &z, 1.0, 0.0).unwrap(), 2.0);
    }

    #[test]
    fn bilinear_reproduces_a_plane_on_a_3x3_grid() {
        let (x, y, z) = grid(3, 3, |a, b| 2.0 * a + 3.0 * b - 1.0);
        close(
            interpolate(&x, &y, &z, 0.25, 1.75).unwrap(),
            2.0 * 0.25 + 3.0 * 1.75 - 1.0,
        );
        close(
            interpolate(&x, &y, &z, 1.5, 0.5).unwrap(),
            2.0 * 1.5 + 3.0 * 0.5 - 1.0,
        );
    }

    #[test]
    fn queries_outside_the_grid_clamp_to_the_boundary() {
        let (x, y, z) = grid(3, 3, |a, b| a + 10.0 * b);
        // (-5, 1.5) clamps to (0, 1.5); (10, 10) clamps to (2, 2).
        close(interpolate(&x, &y, &z, -5.0, 1.5).unwrap(), 15.0);
        close(interpolate(&x, &y, &z, 10.0, 10.0).unwrap(), 22.0);
    }

    #[test]
    fn spline_path_reproduces_a_quadratic_exactly_on_a_5x5_grid() {
        // Akima with three-point end derivatives is exact for quadratics, so
        // the 5x5 spline path must reproduce x² + y² at off-node points.
        let (x, y, z) = grid(5, 5, |a, b| a * a + b * b);
        close(
            interpolate(&x, &y, &z, 1.5, 2.5).unwrap(),
            1.5 * 1.5 + 2.5 * 2.5,
        );
        close(
            interpolate(&x, &y, &z, 0.25, 3.75).unwrap(),
            0.25 * 0.25 + 3.75 * 3.75,
        );
        // Nodes are exact too.
        close(interpolate(&x, &y, &z, 2.0, 3.0).unwrap(), 13.0);
    }

    #[test]
    fn spline_path_clamps_and_stays_exact_on_linear_data() {
        let (x, y, z) = grid(6, 5, |a, b| 3.0 * a - 2.0 * b + 4.0);
        close(
            interpolate(&x, &y, &z, 2.3, 1.7).unwrap(),
            3.0 * 2.3 - 2.0 * 1.7 + 4.0,
        );
        // Outside → clamped to the edge value.
        close(
            interpolate(&x, &y, &z, -3.0, 2.0).unwrap(),
            -2.0 * 2.0 + 4.0,
        );
        close(interpolate(&x, &y, &z, 9.0, 0.0).unwrap(), 3.0 * 5.0 + 4.0);
    }

    #[test]
    fn one_axis_below_five_nodes_stays_bilinear() {
        // 5x4: the Java requires BOTH axes >= 5 for the spline.
        let (x, y, z) = grid(5, 4, |a, b| a * a + b);
        // Bilinear on x² between x=1 and x=2 at 1.5 gives (1+4)/2 = 2.5, not 2.25.
        close(interpolate(&x, &y, &z, 1.5, 1.0).unwrap(), 3.5);
    }

    #[test]
    fn too_small_grids_and_shape_mismatches_are_errors() {
        let err = interpolate(&[0.0], &[0.0, 1.0], &[vec![1.0, 2.0]], 0.0, 0.0)
            .unwrap_err()
            .to_string();
        assert!(err.contains("2x2"), "{err}");
        let err = interpolate(&[0.0, 1.0], &[0.0, 1.0], &[vec![1.0, 2.0]], 0.0, 0.0)
            .unwrap_err()
            .to_string();
        assert!(err.contains("must be 2x2"), "{err}");
    }

    #[test]
    fn spline_path_rejects_non_increasing_axes() {
        let (x, y, z) = grid(5, 5, |a, b| a + b);
        let mut bad_x = x.clone();
        bad_x[2] = bad_x[1]; // duplicate
        let err = interpolate(&bad_x, &y, &z, 1.0, 1.0)
            .unwrap_err()
            .to_string();
        assert!(err.contains("strictly increasing"), "{err}");
    }

    #[test]
    fn nan_query_propagates_on_the_bilinear_path() {
        let (x, y, z) = grid(3, 3, |a, b| a + b);
        assert!(interpolate(&x, &y, &z, f64::NAN, 1.0).unwrap().is_nan());
    }
}
