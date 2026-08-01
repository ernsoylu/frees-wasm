//! Time-domain responses of a SISO LTI system: unit step, impulse and
//! arbitrary forced response (`lsim`).
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/cas/TimeResponse.java`
//! (158 LOC).
//!
//! Per the control-systems architecture, time responses route through the
//! already-tested [`crate::ode::integrator`] rather than a bespoke matrix
//! exponential: a transfer function `num/den` becomes controllable canonical
//! state space via [`super::ss::tf2ss`], the state equation `x' = A x + B u(t)`
//! is integrated with `ode45`, and the trajectory is sampled at the requested
//! times through the integrator's Hermite dense output. The scalar output is
//! `y = C x + D u`.
//!
//! * **step** — `u(t) = 1`, `x(0) = 0`.
//! * **impulse** — `x(0) = B`, `u(t) = 0`: the `C e^{At} B` response. The `D`
//!   direct-feedthrough delta term is not representable on a sampled grid and
//!   is omitted, exactly as the Java omits it.
//! * **lsim** — `x(0) = 0` with the supplied input linearly interpolated
//!   between sample times.
//!
//! # No wall-clock budget
//!
//! The Java passes `System.nanoTime() + 5s` as `OdeProblem.deadlineNanos`.
//! `wasm32-unknown-unknown` has no clock, so [`crate::ode::problem::OdeProblem`]
//! carries no deadline field (its module docs record why) and
//! [`crate::ode::integrator::MAX_STEPS`] is the bound instead. A response that
//! the JVM would abort after five seconds is here bounded by step count.
//!
//! # Guards the Java lacks
//!
//! `responseSS` indexes `t[big - 1]` and `u[i]` without checking either length.
//! An empty `t`, or an `lsim` whose input is shorter than its time vector,
//! throws `ArrayIndexOutOfBoundsException` on the JVM; under `panic = "abort"`
//! the equivalent Rust index would kill the worker. Both are refused here with
//! an explicit message.

use crate::diag::{FreesError, Result};
use crate::linalg::Mat;
use crate::ode::integrator::integrate_and_sample_at;
use crate::ode::problem::{OdeProblem, OdeRhs};

/// Relative tolerance the Java hands `OdeProblem`.
const RESPONSE_RTOL: f64 = 1e-7;
/// Absolute tolerance the Java hands `OdeProblem`.
const RESPONSE_ATOL: f64 = 1e-9;
/// Integrator the Java names.
const RESPONSE_METHOD: &str = "ode45";

fn err(message: impl Into<String>) -> FreesError {
    FreesError::evaluation(message)
}

/// Which time response to compute. Port of `TimeResponse.Kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Step,
    Impulse,
    Lsim,
}

/// Time response of a transfer function `num/den` (descending powers).
/// Port of `TimeResponse.response`.
///
/// `u` is consulted only for [`Kind::Lsim`]; it may be `None` otherwise.
pub fn response(
    kind: Kind,
    num: &[f64],
    den: &[f64],
    u: Option<&[f64]>,
    t: &[f64],
) -> Result<Vec<f64>> {
    let ss = super::ss::tf2ss(num, den)?;
    let n = ss.a.len();
    let b_vec: Vec<f64> = (0..n).map(|i| ss.b[i][0]).collect();
    let c_vec: Vec<f64> = if n > 0 { ss.c[0].clone() } else { Vec::new() };
    response_ss(kind, &ss.a, &b_vec, &c_vec, ss.d[0][0], u, t)
}

/// Time response of a state-space model. Port of `TimeResponse.responseSS`.
pub fn response_ss(
    kind: Kind,
    a: &Mat,
    b: &[f64],
    c: &[f64],
    d: f64,
    u: Option<&[f64]>,
    t: &[f64],
) -> Result<Vec<f64>> {
    let big = t.len();
    let n = a.len();
    if kind == Kind::Lsim && u.is_none_or(|u| u.len() < big) {
        return Err(err(
            "lsim: the input vector u must have at least as many samples as t",
        ));
    }
    let mut y = vec![0.0; big];

    // Pure-gain system (no states): y = D · u.
    if n == 0 {
        for (i, yi) in y.iter_mut().enumerate() {
            *yi = d * input_at(kind, u, i);
        }
        return Ok(y);
    }
    if big == 0 {
        return Err(err("time response: the time vector must not be empty"));
    }
    if a.iter().any(|row| row.len() != n) || b.len() != n || c.len() != n {
        return Err(err(
            "time response: A must be square and B, C must match its dimension",
        ));
    }

    let mut y0 = vec![0.0; n];
    if kind == Kind::Impulse {
        y0.copy_from_slice(b);
    }

    let t0 = t[0];
    let tf = t[big - 1];
    // Degenerate window (single point, or non-increasing times): the integrator
    // requires t0 < tf, so just report the initial output everywhere.
    if tf <= t0 {
        let y_initial = dot(c, &y0);
        for (i, yi) in y.iter_mut().enumerate() {
            *yi = y_initial + d * input_at(kind, u, i);
        }
        return Ok(y);
    }

    // One `impl OdeRhs` per kind, matching the Java's switch: STEP drives with
    // u = 1, IMPULSE with u = 0 (the input lives in x(0) = B instead), LSIM
    // with the interpolated sample vector.
    let rhs_step = |_tt: f64, x: &[f64]| Ok(deriv(a, b, x, 1.0));
    let rhs_impulse = |_tt: f64, x: &[f64]| Ok(deriv(a, b, x, 0.0));
    let u_lsim = u.unwrap_or(&[]);
    let rhs_lsim = |tt: f64, x: &[f64]| Ok(deriv(a, b, x, interp(t, u_lsim, tt)));
    let rhs: &dyn OdeRhs = match kind {
        Kind::Step => &rhs_step,
        Kind::Impulse => &rhs_impulse,
        Kind::Lsim => &rhs_lsim,
    };

    let problem = OdeProblem {
        method: RESPONSE_METHOD.to_string(),
        t0,
        tf,
        y0: y0.clone(),
        rhs,
        points: None,
        fixed_step: None,
        rtol: RESPONSE_RTOL,
        atol: RESPONSE_ATOL,
        max_step: None,
        events: Vec::new(),
    };
    let states = integrate_and_sample_at(&problem, t)?;

    for i in 0..big {
        y[i] = dot(c, &states[i]) + d * input_at(kind, u, i);
    }
    Ok(y)
}

/// `x' = A x + B u` for the controllable canonical (column) `B` vector.
fn deriv(a: &Mat, b: &[f64], x: &[f64], u: f64) -> Vec<f64> {
    let n = x.len();
    let mut dx = vec![0.0; n];
    for i in 0..n {
        let mut s = 0.0;
        for j in 0..n {
            s += a[i][j] * x[j];
        }
        dx[i] = s + b[i] * u;
    }
    dx
}

fn dot(c: &[f64], x: &[f64]) -> f64 {
    let mut s = 0.0;
    for i in 0..c.len() {
        s += c[i] * x[i];
    }
    s
}

/// The driving input value contributed to the output at sample `i`.
fn input_at(kind: Kind, u: Option<&[f64]>, i: usize) -> f64 {
    match kind {
        Kind::Step => 1.0,
        Kind::Impulse => 0.0,
        // The length was checked up front, so this cannot fall back.
        Kind::Lsim => u.and_then(|u| u.get(i).copied()).unwrap_or(0.0),
    }
}

/// Linear interpolation of the sampled input `u(t)` at time `tt`.
/// Port of the private `interp`, binary search included.
fn interp(t: &[f64], u: &[f64], tt: f64) -> f64 {
    let n = t.len();
    if n == 0 || u.is_empty() {
        return 0.0;
    }
    if tt <= t[0] {
        return u[0];
    }
    if tt >= t[n - 1] {
        return u[n - 1];
    }
    let mut lo = 0usize;
    let mut hi = n - 1;
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if t[mid] <= tt {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let span = t[hi] - t[lo];
    if span <= 0.0 {
        return u[lo];
    }
    let w = (tt - t[lo]) / span;
    u[lo] + w * (u[hi] - u[lo])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every expected series was produced by running the **real Java engine**
    /// (`TimeResponse` driven directly against the frEES core jar), so these
    /// compare the whole `tf2ss` → `ode45` → `C x + D u` chain, not just the
    /// arithmetic in this file.
    fn close_slice(actual: &[f64], expected: &[f64], tol: f64, what: &str) {
        assert_eq!(actual.len(), expected.len(), "{what}: length");
        for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
            assert!(
                (a - e).abs() <= tol,
                "{what}[{i}]: got {a}, want {e} (tol {tol})"
            );
        }
    }

    fn grid(n: usize, dt: f64) -> Vec<f64> {
        (0..n).map(|i| i as f64 * dt).collect()
    }

    #[test]
    fn step_response_of_a_first_order_lag() {
        let t = grid(11, 0.5);
        let y = response(Kind::Step, &[0.0, 1.0], &[1.0, 1.0], None, &t).unwrap();
        close_slice(
            &y,
            &[
                0.0,
                0.393_469_403_755_237_6,
                0.632_121_073_660_120_7,
                0.776_870_368_311_044_8,
                0.864_664_718_712_434_7,
                0.917_915_473_189_415,
                0.950_213_380_784_492_2,
                0.969_803_018_715_356,
                0.981_684_552_884_176_9,
                0.988_891_057_581_249_6,
                0.993_262_036_709_483_3,
            ],
            1e-12,
            "step 1/(s+1)",
        );
    }

    #[test]
    fn impulse_response_starts_at_the_b_vector() {
        let t = grid(11, 0.5);
        let y = response(Kind::Impulse, &[0.0, 1.0], &[1.0, 1.0], None, &t).unwrap();
        close_slice(
            &y,
            &[
                1.0,
                0.606_530_442_725_655_2,
                0.367_879_001_621_604_75,
                0.223_130_151_060_903_98,
                0.135_335_169_126_532_23,
                0.082_084_913_694_937_1,
                0.049_787_071_395_772_47,
                0.030_197_362_220_361_89,
                0.018_315_611_786_303_376,
                0.011_108_977_933_402_682,
                0.006_737_947_987_025_695,
            ],
            1e-12,
            "impulse 1/(s+1)",
        );
    }

    #[test]
    fn second_order_step_and_impulse_match_the_oracle() {
        let t = grid(11, 0.5);
        let y = response(Kind::Step, &[0.0, 0.0, 1.0], &[1.0, 1.0, 1.0], None, &t).unwrap();
        close_slice(
            &y,
            &[
                0.0,
                0.104_405_460_958_135_41,
                0.340_299_784_966_943_74,
                0.610_491_727_997_567_8,
                0.849_424_741_142_710_5,
                1.023_359_395_232_797,
                1.124_354_496_951_569_7,
                1.161_649_930_389_738_5,
                1.153_122_797_514_59,
                1.118_446_118_858_662_3,
                1.074_590_580_989_74,
            ],
            1e-11,
            "step 1/(s^2+s+1)",
        );
        let y = response(Kind::Impulse, &[0.0, 0.0, 1.0], &[1.0, 1.0, 1.0], None, &t).unwrap();
        close_slice(
            &y,
            &[
                0.0,
                0.377_345_177_738_892,
                0.533_507_089_387_991_7,
                0.525_424_479_776_871_3,
                0.419_279_660_274_682,
                0.274_110_343_785_469_8,
                0.133_243_143_723_334_26,
                0.022_128_275_366_149_815,
                -0.049_529_861_126_137_734,
                -0.083_448_958_403_567_17,
                -0.087_942_426_980_650_27,
            ],
            1e-11,
            "impulse 1/(s^2+s+1)",
        );
    }

    #[test]
    fn a_biproper_transfer_function_adds_d_times_the_input() {
        // (2s+3)/(s+1) = 2 + 1/(s+1): the step response sits 2 above the lag.
        let t = grid(11, 0.5);
        let y = response(Kind::Step, &[2.0, 3.0], &[1.0, 1.0], None, &t).unwrap();
        let lag = response(Kind::Step, &[0.0, 1.0], &[1.0, 1.0], None, &t).unwrap();
        for i in 0..t.len() {
            assert!(
                (y[i] - (lag[i] + 2.0)).abs() < 1e-12,
                "sample {i}: {} vs {}",
                y[i],
                lag[i] + 2.0
            );
        }
        close_slice(&y[..1], &[2.0], 1e-12, "y(0)");
    }

    #[test]
    fn a_pure_gain_has_no_states_and_no_integration() {
        let t = grid(11, 0.5);
        let y = response(Kind::Step, &[4.0], &[2.0], None, &t).unwrap();
        close_slice(&y, &[2.0; 11], 1e-15, "step gain");
        // Impulse of a pure gain contributes nothing: the delta is not
        // representable on the grid, exactly as the Java documents.
        let y = response(Kind::Impulse, &[4.0], &[2.0], None, &t).unwrap();
        close_slice(&y, &[0.0; 11], 1e-15, "impulse gain");
        // lsim of a pure gain is D * u, sample for sample.
        let u = vec![1.0, 2.0, 3.0];
        let y = response(Kind::Lsim, &[4.0], &[2.0], Some(&u), &[0.0, 1.0, 2.0]).unwrap();
        close_slice(&y, &[2.0, 4.0, 6.0], 1e-15, "lsim gain");
    }

    #[test]
    fn state_space_entry_point_agrees_with_the_transfer_function_one() {
        let t = grid(11, 0.5);
        let via_tf = response(Kind::Step, &[0.0, 1.0], &[1.0, 1.0], None, &t).unwrap();
        let via_ss =
            response_ss(Kind::Step, &vec![vec![-1.0]], &[1.0], &[1.0], 0.0, None, &t).unwrap();
        close_slice(&via_ss, &via_tf, 0.0, "ss vs tf");
    }

    #[test]
    fn impulse_response_of_a_second_order_state_space_model() {
        let t = grid(11, 0.5);
        let y = response_ss(
            Kind::Impulse,
            &vec![vec![0.0, 1.0], vec![-2.0, -3.0]],
            &[0.0, 1.0],
            &[1.0, 0.0],
            0.0,
            None,
            &t,
        )
        .unwrap();
        close_slice(
            &y,
            &[
                0.0,
                0.238_651_255_782_929_73,
                0.232_544_225_686_141_7,
                0.173_343_150_757_831_35,
                0.117_019_663_597_589_45,
                0.075_347_053_877_810_11,
                0.047_308_308_438_144_396,
                0.029_285_497_620_632_633,
                0.017_980_166_213_907_776,
                0.010_985_557_871_237_33,
                0.006_692_547_486_265_661_5,
            ],
            1e-11,
            "impulseSS",
        );
    }

    #[test]
    fn lsim_interpolates_the_input_between_samples() {
        // The adaptive step path differs slightly between the JVM and this
        // port, so a forced response over a long window carries more error
        // than a free one; the oracle values below agree to ~1e-6 relative.
        let t = grid(11, 0.5);
        let u: Vec<f64> = t.iter().map(|v| libm::sin(*v)).collect();
        let y = response(Kind::Lsim, &[0.0, 1.0], &[1.0, 1.0], Some(&u), &t).unwrap();
        close_slice(
            &y,
            &[
                0.0,
                0.102_146_990_355_080_2,
                0.327_730_804_951_717_2,
                0.563_114_850_792_596_8,
                0.715_234_884_667_380_2,
                0.725_368_216_506_466_9,
                0.577_993_459_603_923_8,
                0.301_295_204_885_818_85,
                -0.041_784_353_226_855_8,
                -0.370_150_651_977_399_4,
                -0.605_170_243_257_250_6,
            ],
            5e-6,
            "lsim sin",
        );
    }

    #[test]
    fn lsim_of_a_ramp_into_a_second_order_plant() {
        let t = grid(6, 0.2);
        let u = [0.0, 0.2, 0.4, 0.6, 0.8, 1.0];
        let y = response(Kind::Lsim, &[0.0, 0.0, 1.0], &[1.0, 3.0, 2.0], Some(&u), &t).unwrap();
        close_slice(
            &y,
            &[
                0.0,
                0.001_150_750_319_808_195,
                0.007_987_805_181_341_812,
                0.023_513_109_992_667_282,
                0.048_854_853_534_557_94,
                0.084_045_619_647_588_69,
            ],
            1e-11,
            "lsim ramp",
        );
    }

    #[test]
    fn a_degenerate_time_window_reports_the_initial_output_everywhere() {
        // The integrator requires t0 < tf; a flat or descending grid short-
        // circuits to y = C x(0) + D u, sample for sample.
        let y = response(Kind::Step, &[0.0, 1.0], &[1.0, 1.0], None, &[2.0, 2.0, 2.0]).unwrap();
        close_slice(&y, &[0.0, 0.0, 0.0], 0.0, "flat grid");
        // Impulse starts at x(0) = B, so C·B shows up instead of zero.
        let y = response(Kind::Impulse, &[0.0, 1.0], &[1.0, 1.0], None, &[1.0, 1.0]).unwrap();
        close_slice(&y, &[1.0, 1.0], 0.0, "flat grid, impulse");
    }

    #[test]
    fn interp_clamps_outside_the_sample_grid() {
        let t = [0.0, 1.0, 2.0];
        let u = [10.0, 20.0, 30.0];
        assert_eq!(interp(&t, &u, -5.0), 10.0);
        assert_eq!(interp(&t, &u, 5.0), 30.0);
        assert_eq!(interp(&t, &u, 0.5), 15.0);
        assert_eq!(interp(&t, &u, 1.75), 27.5);
        // Degenerate grids do not divide by zero.
        assert_eq!(interp(&[1.0, 1.0], &[3.0, 4.0], 1.0), 3.0);
        assert_eq!(interp(&[], &[], 1.0), 0.0);
    }

    #[test]
    fn guards_refuse_the_inputs_the_java_indexes_out_of_bounds() {
        // Empty time vector with states present.
        assert!(response(Kind::Step, &[0.0, 1.0], &[1.0, 1.0], None, &[]).is_err());
        // lsim with a shorter input than time vector.
        let t = grid(5, 0.1);
        assert!(response(Kind::Lsim, &[0.0, 1.0], &[1.0, 1.0], Some(&[1.0]), &t).is_err());
        assert!(response(Kind::Lsim, &[0.0, 1.0], &[1.0, 1.0], None, &t).is_err());
        // Mis-shaped state-space input.
        assert!(response_ss(
            Kind::Step,
            &vec![vec![-1.0, 0.0], vec![0.0, -1.0]],
            &[1.0],
            &[1.0, 0.0],
            0.0,
            None,
            &t
        )
        .is_err());
    }

    #[test]
    fn an_improper_transfer_function_is_refused_by_tf2ss() {
        // num longer than den never reaches the integrator.
        assert!(response(
            Kind::Step,
            &[1.0, 2.0, 3.0],
            &[1.0, 1.0],
            None,
            &grid(3, 0.5)
        )
        .is_err());
    }
}
