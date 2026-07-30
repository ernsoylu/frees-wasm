//! Convective heat-transfer correlations for the two-phase component layer.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/props/ConvectiveHeat.java`
//! (120 LOC), in full: single-phase Nusselt numbers (Dittus–Boelter,
//! Gnielinski), the Chen flow-boiling enhancement/suppression factors,
//! condensation Nusselt numbers (Shah, Cavallini–Zecchin), and the §4.8
//! `zone_ramp` smoothing that fades a collapsing moving-boundary zone's
//! heat-transfer and storage terms to zero so the integrator steps *through* a
//! structural event rather than over it.
//!
//! All Nusselt numbers are dimensionless; `zone_ramp` is dimensionless.
//!
//! # Guard polarity
//!
//! Every guard here is written the way the Java writes it — `re <= 0.0`, not
//! `!(re > 0.0)`. That is not an oversight to be tidied: the positive form lets
//! a NaN argument *through* the guard and propagate as NaN, which is what the
//! Java does and what the Newton solver's NaN-residual detection relies on.
//! Flipping them to the NaN-rejecting form would change engine behaviour.

use crate::diag::{FreesError, Result};

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

/// Dittus–Boelter single-phase Nusselt number `0.023·Re^0.8·Pr^n`
/// (`n = 0.4` heating the fluid, `0.3` cooling).
pub fn dittus_boelter(re: f64, pr: f64, n: f64) -> Result<f64> {
    if re <= 0.0 || pr <= 0.0 {
        return Err(FreesError::property(
            "nu_dittus_boelter: Re and Pr must be > 0.",
        ));
    }
    Ok(0.023 * libm::pow(re, 0.8) * libm::pow(pr, n))
}

/// Gnielinski single-phase Nusselt number (more accurate than Dittus–Boelter
/// over `3000 < Re < 5e6`), using the smooth-tube friction factor
/// `f = (0.790·ln Re − 1.64)^-2`.
pub fn gnielinski(re: f64, pr: f64) -> Result<f64> {
    if re <= 0.0 || pr <= 0.0 {
        return Err(FreesError::property(
            "nu_gnielinski: Re and Pr must be > 0.",
        ));
    }
    let f = libm::pow(0.790 * libm::log(re) - 1.64, -2.0);
    let num = (f / 8.0) * (re - 1000.0) * pr;
    let den = 1.0 + 12.7 * libm::sqrt(f / 8.0) * (libm::pow(pr, 2.0 / 3.0) - 1.0);
    Ok(num / den)
}

/// Chen flow-boiling **convective enhancement** factor `F` as a function of the
/// inverse Martinelli parameter `1/X_tt`: `F = 1` for `1/X_tt <= 0.1`, else
/// `2.35·(1/X_tt + 0.213)^0.736`.
pub fn chen_f(xtt: f64) -> Result<f64> {
    if xtt <= 0.0 {
        return Err(FreesError::property(
            "chen_f: Martinelli parameter X_tt must be > 0.",
        ));
    }
    let inv = 1.0 / xtt;
    Ok(if inv <= 0.1 {
        1.0
    } else {
        2.35 * libm::pow(inv + 0.213, 0.736)
    })
}

/// Chen flow-boiling **nucleate-suppression** factor `S` from the liquid
/// Reynolds number and the convective factor `F`:
/// `S = 1 / (1 + 2.53e-6·Re_tp^1.17)`, `Re_tp = Re_l·F^1.25`.
pub fn chen_s(re_l: f64, f: f64) -> Result<f64> {
    if re_l <= 0.0 || f <= 0.0 {
        return Err(FreesError::property("chen_s: Re_l and F must be > 0."));
    }
    let re_tp = re_l * libm::pow(f, 1.25);
    Ok(1.0 / (1.0 + 2.53e-6 * libm::pow(re_tp, 1.17)))
}

/// Shah condensation Nusselt number — the liquid-only Nu boosted by the
/// two-phase factor
///
/// ```text
/// Nu   = Nu_l · [ (1−x)^0.8 + 3.8·x^0.76·(1−x)^0.04 / p_red^0.38 ]
/// Nu_l = 0.023·Re_l^0.8·Pr_l^0.4
/// ```
///
/// with reduced pressure `p_red = P/P_crit`. Quality is clamped to `(0, 1)`.
pub fn shah(re_l: f64, pr_l: f64, x: f64, p_red: f64) -> Result<f64> {
    if re_l <= 0.0 || pr_l <= 0.0 {
        return Err(FreesError::property("nu_shah: Re_l and Pr_l must be > 0."));
    }
    if p_red <= 0.0 || p_red >= 1.0 {
        return Err(FreesError::property(
            "nu_shah: reduced pressure must be in (0,1).",
        ));
    }
    let xx = java_max(1e-6, java_min(1.0 - 1e-6, x));
    let nu_l = 0.023 * libm::pow(re_l, 0.8) * libm::pow(pr_l, 0.4);
    Ok(nu_l
        * (libm::pow(1.0 - xx, 0.8)
            + 3.8 * libm::pow(xx, 0.76) * libm::pow(1.0 - xx, 0.04) / libm::pow(p_red, 0.38)))
}

/// Cavallini–Zecchin condensation Nusselt number
/// `Nu = 0.05·Re_eq^0.8·Pr_l^0.33` with the equivalent Reynolds number
/// `Re_eq = Re_l·[(1−x) + x·(ρ_l/ρ_g)^0.5]`. Quality is clamped to `(0, 1)`.
pub fn cavallini_zecchin(re_l: f64, pr_l: f64, x: f64, rho_l: f64, rho_g: f64) -> Result<f64> {
    if re_l <= 0.0 || pr_l <= 0.0 || rho_l <= 0.0 || rho_g <= 0.0 {
        return Err(FreesError::property(
            "nu_cavallini_zecchin: Re_l, Pr_l and densities must be > 0.",
        ));
    }
    let xx = java_max(1e-6, java_min(1.0 - 1e-6, x));
    let re_eq = re_l * ((1.0 - xx) + xx * libm::sqrt(rho_l / rho_g));
    Ok(0.05 * libm::pow(re_eq, 0.8) * libm::pow(pr_l, 0.33))
}

/// Smooth zone-collapse ramp `tanh(L/ε)` (§4.8): `→1` for a healthy zone,
/// `→0` as the zone length `L` shrinks toward the floor `ε`.
///
/// Multiplying both a moving-boundary zone's heat-transfer and its storage
/// terms by this makes a collapsing zone a true (mass/energy-conserving)
/// passthrough, so the BDF integrates through the event instead of stalling.
pub fn zone_ramp(length: f64, eps: f64) -> Result<f64> {
    if eps <= 0.0 {
        return Err(FreesError::property("zone_ramp: ε must be > 0."));
    }
    Ok(libm::tanh(java_max(0.0, length) / eps))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every expectation is the Java oracle's value (`tools/golden-dumper`).
    fn close(actual: f64, expected: f64) {
        let tol = 1e-14 * libm::fabs(expected).max(1.0);
        assert!(
            libm::fabs(actual - expected) <= tol,
            "expected {expected:.17e}, got {actual:.17e}"
        );
    }

    #[test]
    fn dittus_boelter_matches_the_oracle() {
        close(
            dittus_boelter(20000.0, 4.0, 0.4).unwrap(),
            110.50344792629173,
        );
        close(
            dittus_boelter(20000.0, 4.0, 0.3).unwrap(),
            96.19883883839718,
        );
        close(
            dittus_boelter(1e-6, 1e-6, 0.4).unwrap(),
            1.451201892304443e-09,
        );
        close(dittus_boelter(5e6, 0.7, 0.4).unwrap(), 4559.771245046956);
    }

    #[test]
    fn gnielinski_matches_the_oracle() {
        close(gnielinski(20000.0, 4.0).unwrap(), 118.10259161145514);
        close(gnielinski(3000.0, 0.71).unwrap(), 10.053679639501327);
        close(gnielinski(5e6, 7.0).unwrap(), 18445.769050361047);
        // Outside its band the correlation goes negative — the Java does not
        // guard for that, and neither does this port.
        close(gnielinski(500.0, 4.0).unwrap(), -7.575258971471857);
    }

    #[test]
    fn chen_factors_match_the_oracle() {
        close(chen_f(10.0).unwrap(), 1.0);
        close(chen_f(5.0).unwrap(), 1.2257646719418593);
        close(chen_f(0.5).unwrap(), 4.2167143599552865);
        close(chen_f(1e-6).unwrap(), 61244.618026168544);
        close(chen_s(20000.0, 1.0).unwrap(), 0.785870461356204);
        close(chen_s(20000.0, 4.7).unwrap(), 0.27625808264645985);
        close(chen_s(1e-6, 1e-6).unwrap(), 1.0);
    }

    #[test]
    fn chen_f_is_unity_below_the_threshold() {
        // 1/X_tt <= 0.1 exactly at X_tt = 10, so X_tt = 10 takes the flat arm.
        assert_eq!(chen_f(10.0).unwrap(), 1.0);
        // The correlation is *discontinuous* there: the power-law arm evaluates
        // to ~0.9996 at the threshold, so F steps down as 1/X_tt crosses 0.1
        // before climbing again. That jump is in the Java; do not smooth it.
        let just_past = chen_f(9.999999).unwrap();
        assert!(just_past < 1.0, "{just_past}");
        assert!(just_past > 0.999, "{just_past}");
        // Far past the threshold F rises well above 1.
        assert!(chen_f(1.0).unwrap() > 2.7);
    }

    #[test]
    fn shah_matches_the_oracle() {
        close(shah(20000.0, 3.0, 0.5, 0.2).unwrap(), 452.81804386103465);
        close(shah(20000.0, 3.0, 0.0, 0.2).unwrap(), 98.51078185261377);
        close(shah(20000.0, 3.0, 1.0, 0.2).unwrap(), 397.0031112372945);
        close(
            shah(20000.0, 3.0, 0.5, 0.999999).unwrap(),
            271.52992892305525,
        );
    }

    #[test]
    fn shah_clamps_quality_to_the_open_unit_interval() {
        // x below 0 clamps to 1e-6 (= the x = 0 result); x above 1 clamps to
        // 1 − 1e-6 (= the x = 1 result).
        close(shah(20000.0, 3.0, -5.0, 0.2).unwrap(), 98.51078185261377);
        close(shah(20000.0, 3.0, 5.0, 0.2).unwrap(), 397.0031112372945);
    }

    #[test]
    fn cavallini_zecchin_matches_the_oracle() {
        close(
            cavallini_zecchin(20000.0, 3.0, 0.5, 1200.0, 25.0).unwrap(),
            596.7052209775505,
        );
        close(
            cavallini_zecchin(20000.0, 3.0, 0.0, 1200.0, 25.0).unwrap(),
            198.2650092368316,
        );
        close(
            cavallini_zecchin(20000.0, 3.0, 1.0, 1200.0, 25.0).unwrap(),
            932.6962112268546,
        );
        close(
            cavallini_zecchin(20000.0, 3.0, -1.0, 1200.0, 25.0).unwrap(),
            198.2650092368316,
        );
    }

    #[test]
    fn zone_ramp_matches_the_oracle() {
        close(zone_ramp(0.5, 0.01).unwrap(), 1.0);
        close(zone_ramp(0.001, 0.01).unwrap(), 0.09966799462495582);
        close(zone_ramp(0.0, 0.01).unwrap(), 0.0);
        close(zone_ramp(-1.0, 0.01).unwrap(), 0.0);
        close(zone_ramp(1e-9, 0.01).unwrap(), 9.999999999999968e-08);
    }

    #[test]
    fn guards_carry_the_java_text() {
        assert_eq!(
            dittus_boelter(0.0, 4.0, 0.4).unwrap_err(),
            FreesError::property("nu_dittus_boelter: Re and Pr must be > 0.")
        );
        assert_eq!(
            dittus_boelter(20000.0, -1.0, 0.4).unwrap_err(),
            FreesError::property("nu_dittus_boelter: Re and Pr must be > 0.")
        );
        assert_eq!(
            gnielinski(20000.0, 0.0).unwrap_err(),
            FreesError::property("nu_gnielinski: Re and Pr must be > 0.")
        );
        assert_eq!(
            chen_f(0.0).unwrap_err(),
            FreesError::property("chen_f: Martinelli parameter X_tt must be > 0.")
        );
        assert_eq!(
            chen_s(20000.0, 0.0).unwrap_err(),
            FreesError::property("chen_s: Re_l and F must be > 0.")
        );
        assert_eq!(
            shah(0.0, 3.0, 0.5, 0.2).unwrap_err(),
            FreesError::property("nu_shah: Re_l and Pr_l must be > 0.")
        );
        assert_eq!(
            shah(20000.0, 3.0, 0.5, 1.0).unwrap_err(),
            FreesError::property("nu_shah: reduced pressure must be in (0,1).")
        );
        assert_eq!(
            shah(20000.0, 3.0, 0.5, 0.0).unwrap_err(),
            FreesError::property("nu_shah: reduced pressure must be in (0,1).")
        );
        assert_eq!(
            cavallini_zecchin(20000.0, 3.0, 0.5, 1200.0, 0.0).unwrap_err(),
            FreesError::property("nu_cavallini_zecchin: Re_l, Pr_l and densities must be > 0.")
        );
        assert_eq!(
            zone_ramp(0.5, 0.0).unwrap_err(),
            FreesError::property("zone_ramp: ε must be > 0.")
        );
    }

    #[test]
    fn nan_passes_the_guards_and_propagates() {
        // The Java guards are `re <= 0`, so NaN is *not* rejected — it flows
        // through and the solver sees a NaN residual. Preserve that.
        assert!(dittus_boelter(f64::NAN, 4.0, 0.4).unwrap().is_nan());
        assert!(gnielinski(f64::NAN, 4.0).unwrap().is_nan());
        assert!(chen_f(f64::NAN).unwrap().is_nan());
        assert!(chen_s(f64::NAN, 1.0).unwrap().is_nan());
        assert!(shah(20000.0, 3.0, f64::NAN, 0.2).unwrap().is_nan());
        assert!(cavallini_zecchin(20000.0, 3.0, f64::NAN, 1200.0, 25.0)
            .unwrap()
            .is_nan());
        assert!(zone_ramp(f64::NAN, 0.01).unwrap().is_nan());
    }

    #[test]
    fn java_min_max_keep_nan_and_signed_zero() {
        assert!(java_max(f64::NAN, 1.0).is_nan());
        assert!(java_min(1.0, f64::NAN).is_nan());
        assert!(java_max(0.0, -0.0).is_sign_positive());
        assert!(java_min(0.0, -0.0).is_sign_negative());
    }
}
