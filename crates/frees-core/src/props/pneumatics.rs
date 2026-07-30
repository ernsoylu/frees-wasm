//! Pneumatic (compressible-gas power) constitutive functions — ISO 6358.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/props/Pneumatics.java`
//! (71 LOC), in full.
//!
//! The vocabulary that lets a pneumatic restriction close a mass-flow equation
//! against a pressure drop, so a supply → valve → volume gas circuit has a
//! well-posed operating point for the Newton solver. A pneumatic port is
//! structurally a fluid port (`P` equal, `Σṁ = 0`), so no new connection domain
//! is needed.
//!
//! All SI: pressures in Pa **absolute**, temperatures in K, mass flow in kg/s.

// `b < 0.0 || b >= 1.0` is the Java's guard and is **not** interchangeable with
// clippy's suggested `!(0.0..1.0).contains(&b)`: for `b = NaN` the two disagree.
// Java's pair of comparisons is false (NaN is neither `< 0` nor `>= 1`), so a
// NaN parameter flows through and poisons the result; `Range::contains` is
// `0.0 <= b && b < 1.0`, false for NaN, and its negation would *reject* it.
// Rejecting is arguably nicer, but it is a different engine, so the negated
// form stays and the lint is silenced here rather than in the expression.
#![allow(clippy::manual_range_contains)]

use crate::diag::{FreesError, Result};

/// Reference density at ANR conditions (air, 20 °C, 0.1 MPa, 65 % RH) [kg/m³].
const RHO_ANR: f64 = 1.185;
/// Reference temperature at ANR conditions [K].
const T_ANR: f64 = 293.15;

/// ISO 6358 pneumatic mass flow [kg/s] from an upstream port to a downstream
/// port through a restriction of sonic conductance `c` [m³/(s·Pa)] and critical
/// pressure ratio `b` (typically 0.2–0.5):
///
/// ```text
///   m_choked = C * rho_ANR * P_up * sqrt(T_ANR / T_up)
///   pr = P_down / P_up
///   m = m_choked                                   (pr <= b, choked / sonic)
///   m = m_choked * sqrt(1 - ((pr - b)/(1 - b))^2)  (b < pr < 1, subsonic)
///   m = 0                                          (pr >= 1, no forward flow)
/// ```
///
/// The subsonic factor falls smoothly to 0 as `pr → 1`, so the law is
/// continuous through the choke point and at zero flow — which is what makes it
/// safe for step-halving. The law is **directional** (upstream-defined);
/// reverse flow returns 0. The ANR reference density is air's, because `C` is
/// characterised with air by the standard.
///
/// # Non-physical iterates do not throw
///
/// `p_up <= 0` or `t_up <= 0` returns `Ok(0.0)`, not an error: a Newton iterate
/// may stray into a non-physical state and must be able to step back out of it.
/// Only the two *parameters* (`c`, `b`), which the document fixes and the
/// solver never varies, are hard errors.
pub fn iso6358(c: f64, b: f64, p_up: f64, t_up: f64, p_down: f64) -> Result<f64> {
    if c < 0.0 {
        return Err(FreesError::property(
            "iso6358: sonic conductance C must be >= 0.",
        ));
    }
    if b < 0.0 || b >= 1.0 {
        return Err(FreesError::property(
            "iso6358: critical pressure ratio b must be in [0, 1).",
        ));
    }
    if p_up <= 0.0 || t_up <= 0.0 {
        // A Newton iterate may stray to a non-physical state; return no flow
        // rather than throwing so the solver can step back.
        return Ok(0.0);
    }
    let choked = c * RHO_ANR * p_up * libm::sqrt(T_ANR / t_up);
    let pr = p_down / p_up;
    if pr <= b {
        return Ok(choked);
    }
    if pr >= 1.0 {
        return Ok(0.0);
    }
    let x = (pr - b) / (1.0 - b);
    Ok(choked * libm::sqrt(1.0 - x * x))
}

#[cfg(test)]
mod tests {
    use super::*;

    // A small valve on a 6 bar gauge supply at 20 °C — the fixture's state.
    const C: f64 = 8.5e-9;
    const B: f64 = 0.35;
    const P_UP: f64 = 700000.0;
    const T_UP: f64 = 293.15;

    #[track_caller]
    fn oracle(actual: f64, expected: f64) {
        let diff = (actual - expected).abs();
        assert!(
            diff <= 1e-12 || diff <= 1e-13 * expected.abs().max(actual.abs()),
            "expected {expected}, got {actual} (diff {diff})"
        );
    }

    // --- vs fixtures/golden/pneumatics-iso6358.json -------------------------

    #[test]
    fn choked_flow_matches_the_oracle_and_is_pressure_independent() {
        oracle(iso6358(C, B, P_UP, T_UP, 100000.0).unwrap(), 0.00705075);
        // pr == b exactly is still on the choked side (`pr <= b`).
        oracle(iso6358(C, B, P_UP, T_UP, 245000.0).unwrap(), 0.00705075);
        // Lowering the downstream pressure further changes nothing.
        oracle(iso6358(C, B, P_UP, T_UP, 0.0).unwrap(), 0.00705075);
    }

    #[test]
    fn subsonic_flow_matches_the_oracle() {
        oracle(
            iso6358(C, B, P_UP, T_UP, 500000.0).unwrap(),
            0.005839398199417552,
        );
        oracle(
            iso6358(C, B, P_UP, T_UP, 699000.0).unwrap(),
            0.0004672032562787972,
        );
        // b = 0 makes the whole range subsonic (a pure elliptic law).
        oracle(
            iso6358(C, 0.0, P_UP, T_UP, 500000.0).unwrap(),
            0.004934497086836713,
        );
    }

    #[test]
    fn hot_gas_flows_less_for_the_same_pressure() {
        oracle(
            iso6358(C, B, P_UP, 400.0, 100000.0).unwrap(),
            0.006036014434448213,
        );
    }

    #[test]
    fn no_forward_flow_when_the_pressure_ratio_reaches_one() {
        oracle(iso6358(C, B, P_UP, T_UP, P_UP).unwrap(), 0.0);
        oracle(iso6358(C, B, P_UP, T_UP, 900000.0).unwrap(), 0.0);
    }

    #[test]
    fn a_closed_restriction_passes_nothing() {
        oracle(iso6358(0.0, B, P_UP, T_UP, 100000.0).unwrap(), 0.0);
    }

    #[test]
    fn the_law_is_continuous_through_the_choke_point() {
        // Approaching pr = b from above must converge on the choked value
        // rather than stepping — the property that keeps step-halving sane.
        let choked = iso6358(C, B, P_UP, T_UP, 245000.0).unwrap();
        for eps in [1e-3, 1e-5, 1e-7] {
            let m = iso6358(C, B, P_UP, T_UP, (B + eps) * P_UP).unwrap();
            assert!(m < choked, "{m} should be below the choked {choked}");
            assert!(
                choked - m < 1e-3 * choked,
                "gap {} too large at eps={eps}",
                choked - m
            );
        }
        // ... and to zero as pr -> 1.
        for eps in [1e-3, 1e-6, 1e-9] {
            let m = iso6358(C, B, P_UP, T_UP, (1.0 - eps) * P_UP).unwrap();
            assert!(m > 0.0 && m < 0.06 * choked, "{m} at eps={eps}");
        }
    }

    // --- non-physical iterates and bad parameters ---------------------------

    #[test]
    fn non_physical_upstream_states_return_no_flow_instead_of_failing() {
        oracle(iso6358(C, B, -100.0, T_UP, 50000.0).unwrap(), 0.0);
        oracle(iso6358(C, B, P_UP, -5.0, 50000.0).unwrap(), 0.0);
        oracle(iso6358(C, B, 0.0, T_UP, 50000.0).unwrap(), 0.0);
        oracle(iso6358(C, B, P_UP, 0.0, 50000.0).unwrap(), 0.0);
    }

    #[test]
    fn sonic_conductance_must_not_be_negative() {
        let e = iso6358(-1e-9, B, P_UP, T_UP, 1000.0)
            .unwrap_err()
            .to_string();
        assert!(e.contains("sonic conductance C must be >= 0"), "{e}");
    }

    #[test]
    fn critical_pressure_ratio_must_be_in_zero_to_one() {
        for b in [-0.1, 1.0, 1.5] {
            let e = iso6358(C, b, P_UP, T_UP, 1000.0).unwrap_err().to_string();
            assert!(
                e.contains("critical pressure ratio b must be in [0, 1)"),
                "{e}"
            );
        }
        // NaN takes neither branch of `b < 0 || b >= 1`, exactly as in Java —
        // it flows through and poisons the result instead of being rejected.
        assert!(iso6358(C, f64::NAN, P_UP, T_UP, 1000.0).unwrap().is_nan());
    }

    #[test]
    fn errors_are_property_evaluation_failures() {
        assert!(matches!(
            iso6358(C, 1.0, P_UP, T_UP, 1000.0),
            Err(FreesError::Property { .. })
        ));
    }
}
