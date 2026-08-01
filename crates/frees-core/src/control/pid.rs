//! The interactive PID tuner.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/cas/PidTuner.java`
//! (345 LOC) — the front door `/api/control/pidtune` and `/api/control/plant`
//! call. It composes [`crate::control::design`]'s `pidtune`/`stepinfo`, the
//! polynomial loop algebra (`series`/`feedback`/`margin`) and a step
//! simulation; there is no new control theory here, only orchestration.
//!
//! # What it adds over `design::pidtune`
//!
//! `design::pidtune` returns three gains. This module closes the loop with
//! them, simulates the unit-step response through the same `ode45` the
//! transient path uses, and reports the metrics a tuner UI shows — rise, peak,
//! settling, overshoot, and the realised gain/phase margins. It also owns the
//! two utilities the plant-identification endpoint needs: [`recover_plant`],
//! which backs the open-loop plant out of a linearised closed loop, and
//! [`ss_to_tf`], a numeric Faddeev–LeVerrier conversion that exists precisely
//! because the symbolic `ss2tf` chokes on numerically linearised systems.
//!
//! # Two divergences, both hardening
//!
//! * `Math.clamp` throws when its bounds cross; `f64::clamp` **panics**, which
//!   in wasm aborts the module. [`tune`] returns an error instead — see
//!   `java_clamp`.
//! * `Math.max` propagates NaN, `f64::max` does not. The horizon's lower bound
//!   is `2/max(wc, 1e-9)`, so the difference decides between reporting a bad
//!   `wc` and silently sizing a window from a NaN.

// The Java's own `trimLeadingZeros` here tests `c[i] == 0.0` exactly — unlike
// `PolynomialHelpers`, which uses a `1e-15` band. Substituting a tolerance
// would change which coefficients survive, so the exact comparison stays.
#![allow(clippy::needless_range_loop)]

use crate::control::response::{self, Kind};
use crate::control::{design, tf};
use crate::diag::{FreesError, Result};
use crate::linalg::Mat;

fn err(message: impl Into<String>) -> FreesError {
    FreesError::evaluation(message)
}

/// Tuned gains, the closed-loop step response, and the performance metrics.
/// Port of the Java `PidTuner.Result` record.
#[derive(Debug, Clone, PartialEq)]
pub struct TuneResult {
    /// Proportional gain.
    pub kp: f64,
    /// Integral gain.
    pub ki: f64,
    /// Derivative gain.
    pub kd: f64,
    /// Step-response sample times.
    pub t: Vec<f64>,
    /// Step-response output samples.
    pub y: Vec<f64>,
    /// 10–90 % rise time.
    pub rise_time: f64,
    /// Time of the response peak.
    pub peak_time: f64,
    /// 2 %-band settling time.
    pub settling_time: f64,
    /// Percentage overshoot.
    pub overshoot: f64,
    /// Gain margin in dB (`1e9` when the loop has no phase crossover).
    pub gain_margin: f64,
    /// Phase margin in degrees (`1e9` when the loop has no gain crossover).
    pub phase_margin: f64,
    /// The Java's third `margin` slot — the **gain** crossover frequency.
    pub w_gm: f64,
    /// The Java's fourth `margin` slot — the **phase** crossover frequency.
    pub w_pm: f64,
}

/// Ideal-form controller transfer function `C(s)` as `(num, den)` in
/// descending powers. Port of `PidTuner.controllerTf`.
///
/// * `p` → `Kp / 1`
/// * `pi` → `(Kp·s + Ki) / s`
/// * `pid` → `(Kd·s² + Kp·s + Ki) / s`
pub fn controller_tf(kind: &str, kp: f64, ki: f64, kd: f64) -> Result<(Vec<f64>, Vec<f64>)> {
    match kind {
        "p" => Ok((vec![kp], vec![1.0])),
        "pi" => Ok((vec![kp, ki], vec![1.0, 0.0])),
        "pid" => Ok((vec![kd, kp, ki], vec![1.0, 0.0])),
        other => Err(err(format!("pidtune: unknown controller type '{other}'"))),
    }
}

/// Tune and evaluate. `wc` is the target open-loop gain crossover (rad/s),
/// `pm_deg` the target phase margin; `horizon <= 0` auto-sizes the step
/// window. Port of `PidTuner.tune`.
pub fn tune(
    num: &[f64],
    den: &[f64],
    kind: &str,
    wc: f64,
    pm_deg: f64,
    horizon: f64,
    points: usize,
) -> Result<TuneResult> {
    let [kp, ki, kd] = design::pidtune_pm(num, den, kind, wc, pm_deg)?;

    let (c_num, c_den) = controller_tf(kind, kp, ki, kd)?;
    let (loop_num, loop_den) = tf::series(&c_num, &c_den, num, den); // L = C·G
                                                                     // Unity-feedback closed loop T = L / (1 + L).
    let (closed_num, closed_den) = tf::feedback(&loop_num, &loop_den, &[1.0], &[1.0], 1.0);

    let n = points.max(50);
    let t_end = if horizon > 0.0 {
        horizon
    } else {
        auto_horizon(&closed_den, wc)?
    };
    let t: Vec<f64> = (0..n)
        .map(|i| t_end * i as f64 / (n as f64 - 1.0))
        .collect();
    // A proper closed loop has a shorter numerator; `tf2ss` (inside the step
    // simulator) needs num and den the same length, so left-pad with zeros.
    let padded_num = left_pad(&closed_num, closed_den.len());
    let y = response::response(Kind::Step, &padded_num, &closed_den, None, &t)?;

    let info = design::stepinfo(&t, &y); // rise, peak, settling, overshoot
    let mar = tf::margin(&loop_num, &loop_den); // gm dB, pm, wGc, wPc

    Ok(TuneResult {
        kp,
        ki,
        kd,
        t,
        y,
        rise_time: info[0],
        peak_time: info[1],
        settling_time: info[2],
        overshoot: info[3],
        gain_margin: mar[0],
        phase_margin: mar[1],
        w_gm: mar[2],
        w_pm: mar[3],
    })
}

/// Step-window horizon: the slowest stable closed-loop pole (seven time
/// constants) when there is one, else a multiple of `1/wc`. Port of the
/// private `PidTuner.autoHorizon`.
fn auto_horizon(den: &[f64], wc: f64) -> Result<f64> {
    let fallback = if wc > 0.0 { 12.0 / wc } else { 10.0 };
    // The Java swallows a root-finding failure and falls back.
    let Ok(roots) = tf::roots(den) else {
        return Ok(fallback);
    };
    let mut slowest = f64::INFINITY;
    for r in &roots {
        let re = -r.re; // stable poles have re < 0 → decay rate = −re
        if re > 1e-9 && re < slowest {
            slowest = re;
        }
    }
    if !slowest.is_finite() {
        return Ok(fallback);
    }
    java_clamp(7.0 / slowest, 2.0 / java_max(wc, 1e-9), 1e6)
}

/// A sensible default crossover to seed the response-time slider: the plant's
/// dominant (slowest non-zero) pole magnitude, else the frequency where the
/// plant phase first reaches −90°, else 1 rad/s. Port of
/// `PidTuner.suggestWc`.
pub fn suggest_wc(num: &[f64], den: &[f64]) -> f64 {
    let mut dominant = f64::INFINITY;
    if let Ok(roots) = tf::roots(den) {
        for r in &roots {
            let mag = r.re.hypot(r.im);
            if mag > 1e-9 && mag < dominant {
                dominant = mag;
            }
        }
    }
    if dominant.is_finite() {
        return dominant;
    }
    let log_lo = 1e-4f64.ln();
    let log_hi = 1e4f64.ln();
    let steps = 400;
    for i in 0..steps {
        let w = (log_lo + (log_hi - log_lo) * i as f64 / (steps as f64 - 1.0)).exp();
        let s = design::Cm::new(0.0, w);
        let g = design::Cm::horner(num, s).divide(design::Cm::horner(den, s));
        if g.argument() <= -core::f64::consts::FRAC_PI_2 {
            return w;
        }
    }
    1.0
}

/// Recover the open-loop plant `G(s)` from a linearised closed loop `M` and
/// the controller `C0` currently in it. Port of `PidTuner.recoverPlant`.
///
/// frees' PID error is `e = sp − pv`, so with `L = C0·G`:
/// * reference on `sp` (`e = r − y`): `M = L/(1+L)` ⟹ `L = M/(1−M)`;
/// * reference on `pv` (reverse-acting, `e = y − r`): `M = −L/(1−L)` ⟹
///   `L = M/(M−1)`.
///
/// Then `G = L/C0`, after cancelling the `s^k` factor the recovery introduces.
pub fn recover_plant(
    m_num: &[f64],
    m_den: &[f64],
    c_num: &[f64],
    c_den: &[f64],
    reference_on_sp: bool,
) -> (Vec<f64>, Vec<f64>) {
    let l_num = m_num;
    let l_den = if reference_on_sp {
        subtract_raw(m_den, m_num)
    } else {
        subtract_raw(m_num, m_den)
    };
    // G = L / C0 = (lNum·cDen) / (lDen·cNum)
    let mut g_num = tf::multiply_raw(l_num, c_den);
    let mut g_den = tf::multiply_raw(&l_den, c_num);
    // Cancel the common s^k factor: the controller's integrator (cDen = s) and
    // the type-1 loop's (1−M) each contribute a root at the origin, which must
    // cancel for G to have the right type.
    let drop = common_trailing_zeros(&g_num, &g_den);
    g_num.truncate(g_num.len() - drop);
    g_den.truncate(g_den.len() - drop);
    (trim_leading_zeros(&g_num), trim_leading_zeros(&g_den))
}

/// Count trailing (constant-end) coefficients that are ≈0 in **both**
/// polynomials — the common factor of `s^k` to divide out of a ratio. Port of
/// the private `PidTuner.commonTrailingZeros`.
fn common_trailing_zeros(num: &[f64], den: &[f64]) -> usize {
    let mut scale = 0.0f64;
    for v in num.iter().chain(den.iter()) {
        scale = scale.max(v.abs());
    }
    let tol = if scale == 0.0 { 1.0 } else { scale } * 1e-9;
    let max_drop = num.len().min(den.len()) - 1;
    let mut drop = 0;
    while drop < max_drop
        && num[num.len() - 1 - drop].abs() < tol
        && den[den.len() - 1 - drop].abs() < tol
    {
        drop += 1;
    }
    drop
}

/// SISO state space → transfer function `C(sI−A)⁻¹B + D` by the
/// Faddeev–LeVerrier (Souriau–Frame) recursion. Returns `(num, den)`, both
/// length n+1 in descending powers. Port of `PidTuner.ssToTf`.
///
/// This avoids the symbolic CAS path, which can choke on some
/// numerically-linearised systems.
pub fn ss_to_tf(a: &Mat, b: &[f64], c: &[f64], d: f64) -> (Vec<f64>, Vec<f64>) {
    let n = a.len();
    if n == 0 {
        return (vec![d], vec![1.0]);
    }
    let mut den = vec![0.0; n + 1];
    den[0] = 1.0;
    let mut num_adj = vec![0.0; n]; // C·adj(sI−A)·B, descending sⁿ⁻¹..s⁰
    let mut bk = identity(n); // B₀ = I
    for k in 1..=n {
        // Numerator contribution from B_{k−1}: c·(B_{k−1}·b).
        num_adj[k - 1] = dot(c, &mat_vec(&bk, b));
        let abk = mat_mul_square(a, &bk);
        let pk = -trace(&abk) / k as f64;
        den[k] = pk;
        // B_k = A·B_{k−1} + pk·I
        bk = add_scaled_identity(&abk, pk);
    }
    // G_num = numAdj (deg n−1) + D·den (deg n); pad numAdj to length n+1.
    let mut num = vec![0.0; n + 1];
    num[1..(n + 1)].copy_from_slice(&num_adj[..n]);
    for i in 0..=n {
        num[i] += d * den[i];
    }
    (num, den)
}

// ---------------------------------------------------------------------------
// Small helpers (all transcribed from the Java's private methods)
// ---------------------------------------------------------------------------

fn identity(n: usize) -> Mat {
    let mut m = vec![vec![0.0; n]; n];
    for (i, row) in m.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    m
}

fn mat_mul_square(a: &Mat, b: &Mat) -> Mat {
    let n = a.len();
    let mut r = vec![vec![0.0; n]; n];
    for i in 0..n {
        for k in 0..n {
            let aik = a[i][k];
            if aik == 0.0 {
                continue;
            }
            for j in 0..n {
                r[i][j] += aik * b[k][j];
            }
        }
    }
    r
}

fn mat_vec(m: &Mat, v: &[f64]) -> Vec<f64> {
    m.iter()
        .map(|row| row.iter().zip(v).map(|(a, b)| a * b).sum())
        .collect()
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn trace(m: &Mat) -> f64 {
    m.iter().enumerate().map(|(i, row)| row[i]).sum()
}

fn add_scaled_identity(m: &Mat, scalar: f64) -> Mat {
    let mut r = m.clone();
    for (i, row) in r.iter_mut().enumerate() {
        row[i] += scalar;
    }
    r
}

fn subtract_raw(a: &[f64], b: &[f64]) -> Vec<f64> {
    let neg: Vec<f64> = b.iter().map(|v| -v).collect();
    tf::add_raw(a, &neg)
}

/// Drop leading (highest-order) zero coefficients; keep at least one term.
/// The Java's own `PidTuner.trimLeadingZeros`, which tests for an **exact**
/// zero — not `PolynomialHelpers`' `1e-15` band.
fn trim_leading_zeros(c: &[f64]) -> Vec<f64> {
    let mut i = 0;
    while i + 1 < c.len() && c[i] == 0.0 {
        i += 1;
    }
    c[i..].to_vec()
}

/// Left-pad a coefficient vector with leading zeros to `len`.
fn left_pad(c: &[f64], len: usize) -> Vec<f64> {
    if c.len() >= len {
        return c.to_vec();
    }
    let mut out = vec![0.0; len];
    out[len - c.len()..].copy_from_slice(c);
    out
}

/// `Math.max` — NaN-propagating, unlike Rust's `f64::max`, which returns the
/// non-NaN operand. `auto_horizon` feeds this straight into the clamp bounds,
/// where the difference decides between an error and a silently wrong window.
fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else {
        b
    }
}

/// `Math.clamp(value, min, max)` (Java 21). Rejects `min > max` and NaN
/// bounds, exactly as the JDK does — but as an error rather than an unchecked
/// panic, because `f64::clamp` would abort the wasm module. Reachable with a
/// user-supplied `wc` below `2e-6`, where the horizon's lower bound `2/wc`
/// overtakes its `1e6` ceiling.
fn java_clamp(value: f64, min: f64, max: f64) -> Result<f64> {
    if min.is_nan() {
        return Err(err("pidtune: crossover frequency wc is not a number"));
    }
    if max.is_nan() {
        return Err(err("pidtune: step-horizon ceiling is not a number"));
    }
    if min > max {
        return Err(err(format!(
            "pidtune: crossover frequency wc is too small — the step horizon it \
             demands ({min}) exceeds the {max} ceiling"
        )));
    }
    Ok(max.min(value.max(min)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ground truth from `PidTuner` running inside the real Java engine. The
    /// step responses below therefore also pin the `ode45` port: `tune`
    /// integrates the closed loop, so a drift in the integrator shows up here
    /// as a diff in `y`, `settlingTime` and `overshoot`.
    const TOL: f64 = 1e-9;

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

    #[test]
    fn controller_tf_matches_the_oracle_shapes() {
        let (n, d) = controller_tf("p", 2.0, 0.0, 0.0).unwrap();
        assert_eq!((n, d), (vec![2.0], vec![1.0]));
        let (n, d) = controller_tf("pi", 2.0, 1.0, 0.0).unwrap();
        assert_eq!((n, d), (vec![2.0, 1.0], vec![1.0, 0.0]));
        let (n, d) = controller_tf("pid", 2.0, 1.0, 0.5).unwrap();
        assert_eq!((n, d), (vec![0.5, 2.0, 1.0], vec![1.0, 0.0]));
    }

    #[test]
    fn controller_tf_rejects_an_unknown_type() {
        let e = controller_tf("lead", 1.0, 1.0, 1.0).unwrap_err();
        assert!(e.to_string().contains("unknown controller type"), "{e}");
    }

    #[test]
    fn suggest_wc_matches_the_oracle() {
        // 1/(5s+1): pole at −0.2 → dominant crossover 0.2 rad/s.
        close(suggest_wc(&[2.0], &[5.0, 1.0]), 0.2, 1e-12, "first order");
        // 1/s has no non-zero pole → the phase sweep answers at its first grid
        // point, where arg(1/jw) is already exactly −90°.
        close(
            suggest_wc(&[1.0], &[1.0, 0.0]),
            1.0000000000000009e-4,
            1e-9,
            "pure integrator",
        );
        // 1/((s+1)(s+2)) → dominant pole magnitude 1.
        close(
            suggest_wc(&[1.0], &[1.0, 3.0, 2.0]),
            1.0,
            1e-12,
            "second order",
        );
    }

    #[test]
    fn ss_to_tf_matches_the_oracle() {
        let (n, d) = ss_to_tf(&vec![vec![-1.0]], &[1.0], &[2.0], 0.0);
        vec_close(&n, &[0.0, 2.0], TOL, "ssToTf first order num");
        vec_close(&d, &[1.0, 1.0], TOL, "ssToTf first order den");

        let (n, d) = ss_to_tf(&Vec::new(), &[], &[], 3.5);
        vec_close(&n, &[3.5], TOL, "ssToTf pure gain num");
        vec_close(&d, &[1.0], TOL, "ssToTf pure gain den");

        let (n, d) = ss_to_tf(
            &vec![vec![0.0, 1.0], vec![-2.0, -3.0]],
            &[0.0, 1.0],
            &[1.0, 0.0],
            0.0,
        );
        vec_close(&n, &[0.0, 0.0, 1.0], TOL, "ssToTf two state num");
        vec_close(&d, &[1.0, 3.0, 2.0], TOL, "ssToTf two state den");

        // Non-zero feedthrough: the numerator picks up D·den.
        let (n, d) = ss_to_tf(
            &vec![vec![-1.0, 2.0], vec![0.0, -3.0]],
            &[1.0, 1.0],
            &[1.0, 2.0],
            0.5,
        );
        vec_close(&n, &[0.5, 5.0, 8.5], TOL, "ssToTf with D num");
        vec_close(&d, &[1.0, 4.0, 3.0], TOL, "ssToTf with D den");
    }

    #[test]
    fn recover_plant_matches_the_oracle_for_both_wirings() {
        let (c_num, c_den) = (vec![1.0, 0.5], vec![1.0, 0.0]); // C0 = (s + 0.5)/s
        let (l_num, l_den) = tf::series(&c_num, &c_den, &[2.0], &[5.0, 1.0]);

        // Standard wiring (reference on sp): M = L/(1+L).
        let (m_num, m_den) = tf::feedback(&l_num, &l_den, &[1.0], &[1.0], 1.0);
        let (g_num, g_den) = recover_plant(&m_num, &m_den, &c_num, &c_den, true);
        vec_close(&g_num, &[2.0, 1.0], TOL, "recoverPlant sp num");
        vec_close(&g_den, &[5.0, 3.5, 0.5], TOL, "recoverPlant sp den");
        // …which is G = 2/(5s+1) up to the common (s + 0.5) factor.
        assert_ratio_eq(&[2.0], &[5.0, 1.0], &g_num, &g_den);

        // Reverse wiring (reference on pv): M = −L/(1−L).
        let neg: Vec<f64> = l_num.iter().map(|v| -v).collect();
        let (m_num, m_den) = tf::feedback(&neg, &l_den, &[1.0], &[1.0], 1.0);
        let (g_num, g_den) = recover_plant(&m_num, &m_den, &c_num, &c_den, false);
        vec_close(&g_num, &[-2.0, -1.0], TOL, "recoverPlant pv num");
        vec_close(&g_den, &[-5.0, -3.5, -0.5], TOL, "recoverPlant pv den");
        assert_ratio_eq(&[2.0], &[5.0, 1.0], &g_num, &g_den);
    }

    /// Two transfer functions are equal when their cross-products agree.
    fn assert_ratio_eq(n1: &[f64], d1: &[f64], n2: &[f64], d2: &[f64]) {
        let lhs = tf::multiply_raw(n1, d2);
        let rhs = tf::multiply_raw(n2, d1);
        let scale = lhs
            .iter()
            .chain(rhs.iter())
            .fold(0.0f64, |acc, v| acc.max(v.abs()))
            .max(1.0);
        for i in 0..lhs.len().max(rhs.len()) {
            let a = lhs
                .get(lhs.len().wrapping_sub(1 + i))
                .copied()
                .unwrap_or(0.0);
            let b = rhs
                .get(rhs.len().wrapping_sub(1 + i))
                .copied()
                .unwrap_or(0.0);
            assert!((a - b).abs() / scale < 1e-9, "coefficient {i}: {a} vs {b}");
        }
    }

    // -- tune ---------------------------------------------------------------

    #[test]
    fn tune_pi_first_order_matches_the_oracle() {
        let r = tune(&[2.0], &[5.0, 1.0], "pi", 0.5, 60.0, 0.0, 40).unwrap();
        close(r.kp, 0.832531754730548, TOL, "kp");
        close(r.ki, 0.5290063509461097, TOL, "ki");
        assert_eq!(r.kd, 0.0);
        // `points` is floored at 50 and the horizon is auto-sized from the
        // slowest closed-loop pole.
        assert_eq!(r.t.len(), 50);
        close(r.t[49], 26.265790571780688, TOL, "auto horizon");
        close(r.y[1], 0.18126735568781674, 1e-8, "y[1]");
        close(r.y[12], 1.1555186634775012, 1e-8, "y[12] (peak)");
        close(r.y[49], 1.0007650035437223, 1e-8, "y[49]");
        close(r.rise_time, 2.8300787557190983, 1e-8, "riseTime");
        close(r.peak_time, 6.4324385073748624, TOL, "peakTime");
        close(r.settling_time, 11.050239196324991, 1e-8, "settlingTime");
        close(r.overshoot, 15.463536333284447, 1e-8, "overshoot");
        close(r.gain_margin, 1e9, TOL, "gainMargin sentinel");
        close(r.phase_margin, 60.00049634457612, TOL, "phaseMargin");
        close(r.w_gm, 0.5000119182228188, TOL, "wGm");
        assert_eq!(r.w_pm, 0.0);
    }

    #[test]
    fn tune_pid_matches_the_oracle_at_two_phase_margins() {
        let low = tune(&[1.0], &[1.0, 1.0, 0.0], "pid", 1.0, 40.0, 20.0, 40).unwrap();
        close(low.kp, 1.4088320528055172, TOL, "low kp");
        close(low.ki, 0.7687351979027667, TOL, "low ki");
        close(low.kd, 0.6454783644703281, TOL, "low kd");
        close(low.t[49], 20.0, TOL, "explicit horizon");
        close(low.y[7], 1.3883697552319045, 1e-8, "low y[7] (peak)");
        close(low.rise_time, 1.1579349060970856, 1e-8, "low riseTime");
        close(low.peak_time, 2.857142857142857, TOL, "low peakTime");
        close(
            low.settling_time,
            11.77815684623397,
            1e-8,
            "low settlingTime",
        );
        close(low.overshoot, 38.69406375343488, 1e-8, "low overshoot");
        close(low.phase_margin, 40.00097713151939, TOL, "low phaseMargin");
        close(low.w_gm, 1.0000314685602032, TOL, "low wGm");

        let high = tune(&[1.0], &[1.0, 1.0, 0.0], "pid", 1.0, 70.0, 20.0, 40).unwrap();
        close(high.kp, 1.2817127641115769, TOL, "high kp");
        close(high.ki, 0.4082705424564276, TOL, "high ki");
        close(high.kd, 1.0059430199166672, TOL, "high kd");
        close(high.overshoot, 19.32346164292941, 1e-8, "high overshoot");
        close(
            high.settling_time,
            7.99066037194321,
            1e-8,
            "high settlingTime",
        );
        close(
            high.phase_margin,
            70.00016718194169,
            TOL,
            "high phaseMargin",
        );

        // The robustness knob has to work in the direction it claims.
        assert!(
            high.overshoot <= low.overshoot + 1e-6,
            "a higher target phase margin must not increase overshoot \
             (low = {}, high = {})",
            low.overshoot,
            high.overshoot
        );
    }

    #[test]
    fn tune_p_matches_the_oracle_and_has_no_integral_action() {
        let r = tune(&[2.0], &[5.0, 1.0], "p", 0.5, 60.0, 0.0, 40).unwrap();
        close(r.kp, 1.346291201783626, TOL, "kp");
        assert_eq!((r.ki, r.kd), (0.0, 0.0));
        close(r.t[49], 9.478461459976613, TOL, "auto horizon");
        close(r.y[49], 0.7285218718159606, 1e-8, "y[49]");
        // A pure gain cannot remove steady-state error: the loop settles below 1.
        assert!(r.y[49] < 0.9, "P control should leave an offset");
        close(r.overshoot, 0.0, TOL, "overshoot");
        close(r.phase_margin, 111.80144897430178, TOL, "phaseMargin");
    }

    /// `tune` closes the loop through `control::response`; these are the Java
    /// `TimeResponse.STEP` oracle samples for the same call, which is what
    /// gives the `y` assertions above their teeth.
    #[test]
    fn the_shared_step_simulator_matches_the_oracle() {
        let t: Vec<f64> = (0..41).map(|i| 4.0 * i as f64 / 40.0).collect();
        let y = response::response(
            Kind::Step,
            &[0.0, 0.0, 100.0],
            &[1.0, 15.0, 100.0],
            None,
            &t,
        )
        .unwrap();
        close(y[0], 0.0, TOL, "y[0]");
        close(y[1], 0.29824714647378053, 1e-9, "y[1]");
        close(y[5], 1.0275919056798883, 1e-9, "y[5] (overshoot)");
        close(y[40], 1.000000000716117, 1e-9, "y[40]");

        let y = response::response(Kind::Step, &[0.0, 2.0], &[1.0, 1.0], None, &t).unwrap();
        close(y[1], 0.19032533218823172, 1e-9, "first order y[1]");
        close(y[40], 1.9633686898729246, 1e-9, "first order y[40]");
    }

    /// The Java's `Math.clamp` throws when the horizon's lower bound overtakes
    /// its ceiling; `f64::clamp` would panic, which in wasm is an abort. A
    /// crossover this small is reachable from the API.
    #[test]
    fn tune_reports_an_error_rather_than_panicking_on_a_tiny_crossover() {
        let e = tune(&[2.0], &[5.0, 1.0], "pi", 1e-9, 60.0, 0.0, 50).unwrap_err();
        assert!(e.to_string().contains("too small"), "{e}");
        assert!(java_clamp(1.0, 2.0, 1.0).is_err());
        assert!(java_clamp(1.0, f64::NAN, 1.0).is_err());
        assert!(java_clamp(5.0, 0.0, 1.0).unwrap() == 1.0);
        assert!(java_max(f64::NAN, 1.0).is_nan());
    }

    #[test]
    fn tune_rejects_an_unknown_controller_type() {
        assert!(tune(&[1.0], &[1.0, 1.0], "lead", 1.0, 60.0, 5.0, 50).is_err());
    }

    #[test]
    fn trim_and_pad_follow_the_java_exactly() {
        // PidTuner's own trim uses an EXACT zero test, not a 1e-15 band.
        assert_eq!(trim_leading_zeros(&[0.0, 0.0, 3.0]), vec![3.0]);
        assert_eq!(trim_leading_zeros(&[1e-16, 2.0]), vec![1e-16, 2.0]);
        assert_eq!(trim_leading_zeros(&[0.0, 0.0]), vec![0.0]);
        assert_eq!(left_pad(&[1.0, 2.0], 4), vec![0.0, 0.0, 1.0, 2.0]);
        assert_eq!(left_pad(&[1.0, 2.0], 1), vec![1.0, 2.0]);
    }
}
