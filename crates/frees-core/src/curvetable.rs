//! Curve-table evaluation: piecewise-linear interpolation over `TABLE` blocks.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/core/CurveInterpolator.java`
//! (195 LOC): interpolation along each curve is piecewise-linear, computed in
//! log10 space for log-scaled axes (a straight segment on log paper is a power
//! law, which is what engineering charts encode). For a curve family, the two
//! curves bracketing the parameter are evaluated and blended linearly in the
//! parameter. Arguments outside the tabulated range **clamp to the nearest
//! edge** — empirical charts carry no information beyond their plotted range,
//! and clamping keeps Newton residuals finite instead of blowing up the solve.
//!
//! Beyond the frozen [`lookup`] contract this module carries the rest of the
//! Java class, which the classic-solver table functions (`Interpolate1`,
//! `Lookup`, `LookupRow`, `NLookupRows`, `Differentiate`, `DTable`) consume:
//! [`cubic_lookup`], [`row_count`], [`column`], [`cell`], [`lookup_row`] and
//! [`differentiate`]. The cubic paths port Apache's `SplineInterpolator`
//! (natural cubic spline) exactly.

use crate::diag::{FreesError, Result};
use crate::parser::defs::{Curve, FunctionTableDef};

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

/// Evaluate `table(x)` (lone curve) or `table(x, param)` (curve family,
/// interpolating across curves), honouring the `XLOG`/`YLOG` flags.
///
/// Port of `CurveInterpolator.evaluate`.
pub fn lookup(table: &FunctionTableDef, x: f64, param: Option<f64>) -> Result<f64> {
    let curves = &table.curves;
    if curves.is_empty() {
        return Err(no_curves(table));
    }
    let param = match param {
        None => return interpolate_curve(&curves[0], x, table),
        Some(_) if curves.len() == 1 => return interpolate_curve(&curves[0], x, table),
        Some(p) => p,
    };
    // Sort by family parameter (a missing parameter sorts as 0.0, as in the
    // Java comparator).
    let mut sorted: Vec<&Curve> = curves.iter().collect();
    sorted.sort_by(|a, b| a.param.unwrap_or(0.0).total_cmp(&b.param.unwrap_or(0.0)));
    let first = sorted[0];
    let last = sorted[sorted.len() - 1];
    let (Some(first_param), Some(last_param)) = (first.param, last.param) else {
        return Err(FreesError::evaluation(format!(
            "Function table '{}' is called with two arguments but its curves \
             have no parameter values.",
            table.name
        )));
    };
    if param <= first_param {
        return interpolate_curve(first, x, table);
    }
    if param >= last_param {
        return interpolate_curve(last, x, table);
    }
    for i in 1..sorted.len() {
        let hi = sorted[i];
        let hi_param = curve_param(hi, table)?;
        if param <= hi_param {
            let lo = sorted[i - 1];
            let lo_param = curve_param(lo, table)?;
            let y_lo = interpolate_curve(lo, x, table)?;
            let y_hi = interpolate_curve(hi, x, table)?;
            let t = (param - lo_param) / (hi_param - lo_param);
            return Ok(y_lo + t * (y_hi - y_lo));
        }
    }
    interpolate_curve(last, x, table)
}

/// A family curve's parameter; a missing one mid-family is a
/// `NullPointerException` in Java, an explicit error here.
fn curve_param(curve: &Curve, table: &FunctionTableDef) -> Result<f64> {
    curve.param.ok_or_else(|| {
        FreesError::evaluation(format!(
            "Function table '{}' mixes curves with and without parameter values.",
            table.name
        ))
    })
}

fn no_curves(table: &FunctionTableDef) -> FreesError {
    FreesError::evaluation(format!("Function table '{}' has no curves.", table.name))
}

/// Number of data rows (length of the longest curve).
///
/// Port of `CurveInterpolator.rowCount`.
pub fn row_count(table: &FunctionTableDef) -> usize {
    table.curves.iter().map(|c| c.xs.len()).max().unwrap_or(0)
}

/// A whole 1-based column: column 1 is the x axis, columns 2.. are the
/// curves' y values.
///
/// Port of `CurveInterpolator.column`.
pub fn column(table: &FunctionTableDef, col: i64) -> Result<Vec<f64>> {
    let curves = &table.curves;
    let n_cols = 1 + curves.len() as i64;
    if col < 1 || col > n_cols {
        return Err(FreesError::evaluation(format!(
            "Table '{}': column {col} out of range 1..{n_cols}.",
            table.name
        )));
    }
    if curves.is_empty() {
        // Java reaches curves.get(0) and crashes; refuse explicitly.
        return Err(no_curves(table));
    }
    let reference = &curves[if col == 1 { 0 } else { (col - 2) as usize }];
    Ok(if col == 1 {
        reference.xs.clone()
    } else {
        reference.ys.clone()
    })
}

/// 1-based cell value at `(row, col)`.
///
/// Port of `CurveInterpolator.cell`.
pub fn cell(table: &FunctionTableDef, row: i64, col: i64) -> Result<f64> {
    let data = column(table, col)?;
    if row < 1 || row > data.len() as i64 {
        return Err(FreesError::evaluation(format!(
            "Table '{}': row {row} out of range 1..{}.",
            table.name,
            data.len()
        )));
    }
    Ok(data[(row - 1) as usize])
}

/// Fractional 1-based row where column `col` crosses `val` (linear).
///
/// Port of `CurveInterpolator.lookupRow`.
pub fn lookup_row(table: &FunctionTableDef, col: i64, val: f64) -> Result<f64> {
    let data = column(table, col)?;
    for (i, &d) in data.iter().enumerate() {
        if d == val {
            return Ok(i as f64 + 1.0);
        }
    }
    for i in 1..data.len() {
        let a = data[i - 1];
        let b = data[i];
        if (val >= a && val <= b) || (val <= a && val >= b) {
            let t = if b == a { 0.0 } else { (val - a) / (b - a) };
            return Ok(i as f64 + t);
        }
    }
    Err(FreesError::evaluation(format!(
        "LookupRow: value {val} not found in column {col} of table '{}'.",
        table.name
    )))
}

/// dy/dx at `x_val` from columns `(x_col, y_col)`; the cubic form uses the
/// natural-spline derivative.
///
/// Port of `CurveInterpolator.differentiate`.
pub fn differentiate(
    table: &FunctionTableDef,
    y_col: i64,
    x_col: i64,
    x_val: f64,
    cubic: bool,
) -> Result<f64> {
    let xs = column(table, x_col)?;
    let ys = column(table, y_col)?;
    if xs.len() != ys.len() {
        // Java would index off the shorter array; refuse explicitly.
        return Err(FreesError::evaluation(format!(
            "Table '{}': columns {x_col} and {y_col} have different lengths.",
            table.name
        )));
    }
    // Stable sort by x, exactly like the Java's boxed comparator sort.
    let mut order: Vec<usize> = (0..xs.len()).collect();
    order.sort_by(|&a, &b| xs[a].total_cmp(&xs[b]));
    let sx: Vec<f64> = order.iter().map(|&i| xs[i]).collect();
    let sy: Vec<f64> = order.iter().map(|&i| ys[i]).collect();
    if cubic && sx.len() >= 3 {
        let clamped = java_max(sx[0], java_min(sx[sx.len() - 1], x_val));
        let spline = natural_spline(&sx, &sy, &table.name)?;
        return Ok(spline.derivative_at(clamped));
    }
    if sx.len() < 2 {
        // Java indexes sx[1] and crashes on a one-point table.
        return Err(FreesError::evaluation(format!(
            "Table '{}': differentiation needs at least two rows.",
            table.name
        )));
    }
    let mut hi = 1;
    while hi < sx.len() - 1 && sx[hi] < x_val {
        hi += 1;
    }
    let dx = sx[hi] - sx[hi - 1];
    Ok(if dx == 0.0 {
        0.0
    } else {
        (sy[hi] - sy[hi - 1]) / dx
    })
}

/// Cubic-spline interpolation of the table's first curve (1-D). Falls back to
/// piecewise-linear when there are fewer than three points. Arguments outside
/// the tabulated range clamp to the nearest edge, matching [`lookup`].
/// Note the Java ignores the `XLOG`/`YLOG` flags on this path, and so does
/// this port.
///
/// Port of `CurveInterpolator.cubicEvaluate`.
pub fn cubic_lookup(table: &FunctionTableDef, x: f64) -> Result<f64> {
    let curves = &table.curves;
    if curves.is_empty() {
        return Err(no_curves(table));
    }
    let curve = &curves[0];
    let xs = &curve.xs;
    let ys = &curve.ys;
    if xs.len() < 3 {
        return interpolate_curve(curve, x, table);
    }
    if x <= xs[0] {
        return Ok(ys[0]);
    }
    if x >= xs[xs.len() - 1] {
        return Ok(ys[ys.len() - 1]);
    }
    let spline = natural_spline(xs, ys, &table.name)?;
    Ok(spline.value_at(x))
}

/// Port of the private `CurveInterpolator.interpolate`: piecewise-linear along
/// one curve, in log10 space per axis flag, clamped at both ends.
fn interpolate_curve(curve: &Curve, x: f64, table: &FunctionTableDef) -> Result<f64> {
    let xs = &curve.xs;
    let ys = &curve.ys;
    if xs.is_empty() {
        return Err(FreesError::evaluation(format!(
            "Function table '{}' has an empty curve.",
            table.name
        )));
    }
    if xs.len() == 1 || x <= xs[0] {
        return Ok(ys[0]);
    }
    if x >= xs[xs.len() - 1] {
        return Ok(ys[ys.len() - 1]);
    }
    let mut hi = 1;
    while xs[hi] < x {
        hi += 1;
    }
    let x0 = scale(xs[hi - 1], table.x_log);
    let x1 = scale(xs[hi], table.x_log);
    let y0 = scale(ys[hi - 1], table.y_log);
    let y1 = scale(ys[hi], table.y_log);
    let t = if x1 == x0 {
        0.0
    } else {
        (scale(x, table.x_log) - x0) / (x1 - x0)
    };
    Ok(unscale(y0 + t * (y1 - y0), table.y_log))
}

fn scale(v: f64, log: bool) -> f64 {
    if log {
        libm::log10(v)
    } else {
        v
    }
}

fn unscale(v: f64, log: bool) -> f64 {
    if log {
        libm::pow(10.0, v)
    } else {
        v
    }
}

// ---------------------------------------------------------------------------
// Natural cubic spline (Apache `SplineInterpolator` port)
// ---------------------------------------------------------------------------

/// The four per-segment coefficient arrays of a natural cubic spline:
/// `p_i(t) = y_i + b_i·t + c_i·t² + d_i·t³` with `t = x − knots[i]`.
struct NaturalSpline {
    knots: Vec<f64>,
    b: Vec<f64>,
    c: Vec<f64>,
    d: Vec<f64>,
    y: Vec<f64>,
}

/// Exact port of `SplineInterpolator.interpolate` (n ≥ 3 points, strictly
/// increasing knots — the Java throws `NonMonotonicSequenceException`
/// otherwise).
// `!(w[1] > w[0])` rather than `w[1] <= w[0]`: the negated form also rejects a
// NaN knot, which is exactly what the Java monotonicity check does.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn natural_spline(x: &[f64], y: &[f64], table_name: &str) -> Result<NaturalSpline> {
    let n = x.len() - 1;
    debug_assert!(x.len() >= 3 && x.len() == y.len());
    for w in x.windows(2) {
        if !(w[1] > w[0]) {
            return Err(FreesError::evaluation(format!(
                "Table '{table_name}': x values must be strictly increasing \
                 for cubic interpolation."
            )));
        }
    }
    let h: Vec<f64> = (0..n).map(|i| x[i + 1] - x[i]).collect();
    let mut mu = vec![0.0_f64; n];
    let mut z = vec![0.0_f64; n + 1];
    for i in 1..n {
        let g = 2.0 * (x[i + 1] - x[i - 1]) - h[i - 1] * mu[i - 1];
        mu[i] = h[i] / g;
        z[i] = (3.0 * (y[i + 1] * h[i - 1] - y[i] * (x[i + 1] - x[i - 1]) + y[i - 1] * h[i])
            / (h[i - 1] * h[i])
            - h[i - 1] * z[i - 1])
            / g;
    }
    let mut b = vec![0.0_f64; n];
    let mut c = vec![0.0_f64; n + 1];
    let mut d = vec![0.0_f64; n];
    for j in (0..n).rev() {
        c[j] = z[j] - mu[j] * c[j + 1];
        b[j] = (y[j + 1] - y[j]) / h[j] - h[j] * (c[j + 1] + 2.0 * c[j]) / 3.0;
        d[j] = (c[j + 1] - c[j]) / (3.0 * h[j]);
    }
    Ok(NaturalSpline {
        knots: x.to_vec(),
        b,
        c,
        d,
        y: y.to_vec(),
    })
}

impl NaturalSpline {
    fn value_at(&self, v: f64) -> f64 {
        let i = crate::interp2::spline_interval(&self.knots, v);
        let t = v - self.knots[i];
        self.y[i] + t * (self.b[i] + t * (self.c[i] + t * self.d[i]))
    }

    fn derivative_at(&self, v: f64) -> f64 {
        let i = crate::interp2::spline_interval(&self.knots, v);
        let t = v - self.knots[i];
        self.b[i] + t * (2.0 * self.c[i] + t * 3.0 * self.d[i])
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

    fn table(curves: Vec<Curve>, x_log: bool, y_log: bool) -> FunctionTableDef {
        FunctionTableDef {
            name: "t".into(),
            arg_names: vec!["x".into()],
            x_log,
            y_log,
            curves,
            output_unit: None,
            arg_units: None,
        }
    }

    fn curve(param: Option<f64>, xs: &[f64], ys: &[f64]) -> Curve {
        Curve {
            param,
            xs: xs.to_vec(),
            ys: ys.to_vec(),
        }
    }

    // -- lone-curve linear interpolation ------------------------------------

    #[test]
    fn lone_curve_interpolates_linearly_and_hits_nodes_exactly() {
        let t = table(
            vec![curve(None, &[0.0, 1.0, 3.0], &[0.0, 10.0, 30.0])],
            false,
            false,
        );
        close(lookup(&t, 0.5, None).unwrap(), 5.0);
        close(lookup(&t, 2.0, None).unwrap(), 20.0);
        close(lookup(&t, 1.0, None).unwrap(), 10.0);
    }

    #[test]
    fn out_of_range_arguments_clamp_to_the_edges() {
        let t = table(vec![curve(None, &[1.0, 2.0], &[5.0, 9.0])], false, false);
        close(lookup(&t, 0.0, None).unwrap(), 5.0);
        close(lookup(&t, 100.0, None).unwrap(), 9.0);
    }

    #[test]
    fn a_single_point_curve_always_returns_its_value() {
        let t = table(vec![curve(None, &[2.0], &[7.5])], false, false);
        close(lookup(&t, -10.0, None).unwrap(), 7.5);
        close(lookup(&t, 2.0, None).unwrap(), 7.5);
        close(lookup(&t, 99.0, None).unwrap(), 7.5);
    }

    #[test]
    fn log_log_interpolation_follows_the_power_law() {
        // y = x² sampled at decades: a straight line on log-log paper, so the
        // midpoint in log space (x = 10^1.5) must give exactly 10^3.
        let t = table(
            vec![curve(None, &[1.0, 10.0, 100.0], &[1.0, 100.0, 10000.0])],
            true,
            true,
        );
        close(lookup(&t, libm::pow(10.0, 1.5), None).unwrap(), 1000.0);
    }

    #[test]
    fn xlog_only_interpolates_the_y_axis_linearly_in_log_x() {
        let t = table(vec![curve(None, &[1.0, 100.0], &[0.0, 2.0])], true, false);
        // Halfway in log10(x) at x=10 → y = 1.
        close(lookup(&t, 10.0, None).unwrap(), 1.0);
    }

    // -- curve families ------------------------------------------------------

    #[test]
    fn family_blends_linearly_between_bracketing_curves() {
        let t = table(
            vec![
                curve(Some(1.0), &[0.0, 1.0], &[0.0, 10.0]),
                curve(Some(3.0), &[0.0, 1.0], &[0.0, 30.0]),
            ],
            false,
            false,
        );
        close(lookup(&t, 1.0, Some(2.0)).unwrap(), 20.0); // midway
        close(lookup(&t, 1.0, Some(1.0)).unwrap(), 10.0); // exactly on a curve
        close(lookup(&t, 1.0, Some(0.0)).unwrap(), 10.0); // below range → first
        close(lookup(&t, 1.0, Some(9.0)).unwrap(), 30.0); // above range → last
        close(lookup(&t, 0.5, Some(2.0)).unwrap(), 10.0); // blend of interpolated values
    }

    #[test]
    fn family_curves_sort_by_parameter_before_blending() {
        // Declared out of order; param 2 must still blend curves 1 and 3.
        let t = table(
            vec![
                curve(Some(3.0), &[0.0, 1.0], &[0.0, 30.0]),
                curve(Some(1.0), &[0.0, 1.0], &[0.0, 10.0]),
            ],
            false,
            false,
        );
        close(lookup(&t, 1.0, Some(2.0)).unwrap(), 20.0);
    }

    #[test]
    fn param_on_a_single_curve_table_uses_that_curve() {
        let t = table(vec![curve(None, &[0.0, 1.0], &[0.0, 10.0])], false, false);
        close(lookup(&t, 0.5, Some(42.0)).unwrap(), 5.0);
    }

    #[test]
    fn family_without_parameters_rejects_two_argument_calls() {
        let t = table(
            vec![
                curve(None, &[0.0, 1.0], &[0.0, 1.0]),
                curve(None, &[0.0, 1.0], &[1.0, 2.0]),
            ],
            false,
            false,
        );
        let err = lookup(&t, 0.5, Some(1.0)).unwrap_err().to_string();
        assert!(err.contains("no parameter values"), "{err}");
    }

    #[test]
    fn empty_tables_and_empty_curves_are_errors() {
        let t = table(vec![], false, false);
        assert!(lookup(&t, 1.0, None)
            .unwrap_err()
            .to_string()
            .contains("has no curves"));
        let t = table(vec![curve(None, &[], &[])], false, false);
        assert!(lookup(&t, 1.0, None)
            .unwrap_err()
            .to_string()
            .contains("empty curve"));
    }

    // -- row/column/cell accessors ------------------------------------------

    fn two_curve_table() -> FunctionTableDef {
        table(
            vec![
                curve(Some(1.0), &[0.0, 1.0, 2.0], &[5.0, 6.0, 7.0]),
                curve(Some(2.0), &[0.0, 1.0, 2.0], &[50.0, 60.0, 70.0]),
            ],
            false,
            false,
        )
    }

    #[test]
    fn row_count_is_the_longest_curve() {
        assert_eq!(row_count(&two_curve_table()), 3);
        assert_eq!(row_count(&table(vec![], false, false)), 0);
    }

    #[test]
    fn column_one_is_x_and_later_columns_are_curve_ys() {
        let t = two_curve_table();
        assert_eq!(column(&t, 1).unwrap(), vec![0.0, 1.0, 2.0]);
        assert_eq!(column(&t, 2).unwrap(), vec![5.0, 6.0, 7.0]);
        assert_eq!(column(&t, 3).unwrap(), vec![50.0, 60.0, 70.0]);
        let err = column(&t, 4).unwrap_err().to_string();
        assert!(err.contains("column 4 out of range 1..3"), "{err}");
        let err = column(&t, 0).unwrap_err().to_string();
        assert!(err.contains("out of range"), "{err}");
    }

    #[test]
    fn cell_is_one_based_and_bounds_checked() {
        let t = two_curve_table();
        close(cell(&t, 1, 1).unwrap(), 0.0);
        close(cell(&t, 3, 3).unwrap(), 70.0);
        let err = cell(&t, 4, 1).unwrap_err().to_string();
        assert!(err.contains("row 4 out of range 1..3"), "{err}");
    }

    #[test]
    fn lookup_row_finds_exact_and_interpolated_crossings() {
        let t = two_curve_table();
        close(lookup_row(&t, 2, 6.0).unwrap(), 2.0); // exact hit
        close(lookup_row(&t, 2, 6.5).unwrap(), 2.5); // halfway between rows 2 and 3
        let err = lookup_row(&t, 2, 100.0).unwrap_err().to_string();
        assert!(err.contains("not found in column 2"), "{err}");
    }

    #[test]
    fn lookup_row_handles_descending_columns() {
        let t = table(vec![curve(None, &[0.0, 1.0], &[10.0, 4.0])], false, false);
        close(lookup_row(&t, 2, 7.0).unwrap(), 1.5);
    }

    // -- differentiate & cubic -----------------------------------------------

    fn parabola_table() -> FunctionTableDef {
        // y = x² sampled at 0, 1, 2.
        table(
            vec![curve(None, &[0.0, 1.0, 2.0], &[0.0, 1.0, 4.0])],
            false,
            false,
        )
    }

    #[test]
    fn linear_differentiate_returns_segment_slopes() {
        let t = parabola_table();
        close(differentiate(&t, 2, 1, 0.5, false).unwrap(), 1.0);
        close(differentiate(&t, 2, 1, 1.5, false).unwrap(), 3.0);
        // At or below the first node the first segment's slope applies.
        close(differentiate(&t, 2, 1, -1.0, false).unwrap(), 1.0);
    }

    #[test]
    fn cubic_differentiate_matches_the_natural_spline_derivative() {
        // Natural cubic spline through (0,0), (1,1), (2,4):
        // b = [0.5, 2], c = [0, 1.5, 0], d = [0.5, -0.5] →
        // p'(1) = b₁ = 2, p'(0.5) = 0.5 + 3·0.5·0.25 = 0.875.
        let t = parabola_table();
        close(differentiate(&t, 2, 1, 1.0, true).unwrap(), 2.0);
        close(differentiate(&t, 2, 1, 0.5, true).unwrap(), 0.875);
        // Clamped outside the range: derivative at the nearest edge.
        close(differentiate(&t, 2, 1, -5.0, true).unwrap(), 0.5);
        close(differentiate(&t, 2, 1, 99.0, true).unwrap(), 3.5);
    }

    #[test]
    fn differentiate_sorts_rows_by_x_first() {
        let t = table(
            vec![curve(None, &[2.0, 0.0, 1.0], &[4.0, 0.0, 1.0])],
            false,
            false,
        );
        close(differentiate(&t, 2, 1, 0.5, false).unwrap(), 1.0);
        close(differentiate(&t, 2, 1, 1.0, true).unwrap(), 2.0);
    }

    #[test]
    fn cubic_lookup_evaluates_the_natural_spline_between_clamped_edges() {
        // Same spline: p(0.5) = 0.25·? → y = 0 + 0.5·0.5 + 0·0.25 + 0.5·0.125 = 0.3125.
        let t = parabola_table();
        close(cubic_lookup(&t, 0.5).unwrap(), 0.3125);
        close(cubic_lookup(&t, 1.0).unwrap(), 1.0);
        close(cubic_lookup(&t, -3.0).unwrap(), 0.0);
        close(cubic_lookup(&t, 7.0).unwrap(), 4.0);
    }

    #[test]
    fn cubic_lookup_falls_back_to_linear_below_three_points() {
        let t = table(vec![curve(None, &[0.0, 2.0], &[0.0, 4.0])], false, false);
        close(cubic_lookup(&t, 1.0).unwrap(), 2.0);
    }

    #[test]
    fn duplicate_x_values_reject_cubic_interpolation() {
        let t = table(
            vec![curve(None, &[0.0, 1.0, 1.0, 2.0], &[0.0, 1.0, 1.5, 4.0])],
            false,
            false,
        );
        let err = cubic_lookup(&t, 0.5).unwrap_err().to_string();
        assert!(err.contains("strictly increasing"), "{err}");
        let err = differentiate(&t, 2, 1, 0.5, true).unwrap_err().to_string();
        assert!(err.contains("strictly increasing"), "{err}");
    }

    #[test]
    fn one_point_tables_cannot_be_differentiated() {
        let t = table(vec![curve(None, &[1.0], &[2.0])], false, false);
        let err = differentiate(&t, 2, 1, 1.0, false).unwrap_err().to_string();
        assert!(err.contains("at least two rows"), "{err}");
    }
}
