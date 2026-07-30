//! Ideal-gas (perfect-gas) compressible-flow relations.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/props/CompressibleFlow.java`
//! (340 LOC), in full: isentropic stagnation/area ratios, normal shock,
//! Rayleigh flow, Fanno flow, the Prandtl–Meyer expansion and the oblique-shock
//! θ–β–M relation.
//!
//! Every ratio is dimensionless. **Angles are radians** — the SI angle unit
//! frees uses for its trigonometric intrinsics, so the results compose directly
//! with `sin`/`tan`.
//!
//! The forward relations are pure functions of Mach number `m` and the ratio of
//! specific heats `k`; the engine's Newton solver inverts them numerically when
//! an unknown appears inside one. The three genuinely multi-valued inverses
//! ([`mach_from_a_over_astar`], [`mach_from_prandtl_meyer`], [`beta_oblique`])
//! are provided explicitly with a branch selector, because Newton alone cannot
//! choose between two roots. They share the Java's [`bisect`] — **200
//! iterations, `|f| < 1e-12` or a bracket narrower than `1e-12`** — and the
//! iteration scheme is load-bearing for parity: a different bracket or stopping
//! rule moves the last digits and the golden corpus notices.
//!
//! All transcendentals go through [`libm`] so native and wasm runs agree bit
//! for bit (the crate-wide determinism rule).

// The Java's domain guards are written `!(x > 0.0)` so that a NaN argument
// takes the reject branch, which `x <= 0.0` would not. Clippy's
// `neg_cmp_op_on_partial_ord` exists to catch the *accidental* form; here the
// NaN behaviour is the point, and it matches the Java guards being ported.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::diag::{FreesError, Result};

fn err(message: impl Into<String>) -> FreesError {
    FreesError::property(message)
}

fn require_k(k: f64) -> Result<()> {
    if !(k > 1.0) {
        return Err(err(format!(
            "Compressible-flow: ratio of specific heats k must be > 1, got {k}."
        )));
    }
    Ok(())
}

fn require_mach(m: f64) -> Result<()> {
    if !(m > 0.0) {
        return Err(err(format!(
            "Compressible-flow: Mach number must be > 0, got {m}."
        )));
    }
    Ok(())
}

fn require_supersonic(what: &str, m: f64) -> Result<()> {
    if !(m >= 1.0) {
        return Err(err(format!(
            "Compressible-flow: {what} requires a supersonic Mach number M >= 1, got {m}."
        )));
    }
    Ok(())
}

// ----- Isentropic flow (stagnation / area ratios) ---------------------------

/// Stagnation-to-static temperature ratio `T0/T = 1 + (k−1)/2 · M²`.
pub fn t0_over_t(m: f64, k: f64) -> Result<f64> {
    require_mach(m)?;
    require_k(k)?;
    Ok(1.0 + 0.5 * (k - 1.0) * m * m)
}

/// Stagnation-to-static pressure ratio `P0/P`.
pub fn p0_over_p(m: f64, k: f64) -> Result<f64> {
    Ok(libm::pow(t0_over_t(m, k)?, k / (k - 1.0)))
}

/// Stagnation-to-static density ratio `rho0/rho`.
pub fn rho0_over_rho(m: f64, k: f64) -> Result<f64> {
    Ok(libm::pow(t0_over_t(m, k)?, 1.0 / (k - 1.0)))
}

/// Isentropic area ratio `A/A*` (1 at M = 1, increasing away from sonic).
pub fn a_over_astar(m: f64, k: f64) -> Result<f64> {
    require_mach(m)?;
    require_k(k)?;
    let t = 1.0 + 0.5 * (k - 1.0) * m * m;
    let exponent = (k + 1.0) / (2.0 * (k - 1.0));
    Ok((1.0 / m) * libm::pow((2.0 / (k + 1.0)) * t, exponent))
}

/// Inverts `A/A*` for Mach number on the requested branch.
///
/// `regime` is matched by prefix, case- and whitespace-insensitively:
/// `"sub"`/`"subsonic"` selects M < 1, `"sup"`/`"supersonic"` selects M > 1.
/// The brackets are the Java's: `[1e-6, 1]` subsonic, `[1, 50]` supersonic.
pub fn mach_from_a_over_astar(ratio: f64, k: f64, regime: &str) -> Result<f64> {
    require_k(k)?;
    if ratio < 1.0 {
        return Err(err(format!(
            "Compressible-flow: A/A* must be >= 1, got {ratio}."
        )));
    }
    let r = regime.trim().to_lowercase();
    let subsonic = r.starts_with("sub");
    let supersonic = r.starts_with("sup");
    if !subsonic && !supersonic {
        return Err(err(format!(
            "Compressible-flow: mach_A_Astar branch must be 'subsonic' or 'supersonic', \
             got '{regime}'."
        )));
    }
    if ratio == 1.0 {
        return Ok(1.0);
    }
    let lo = if subsonic { 1e-6 } else { 1.0 };
    let hi = if subsonic { 1.0 } else { 50.0 };
    // A/A* is monotone on each branch (decreasing for M<1, increasing for M>1).
    bisect(|m| Ok(a_over_astar(m, k)? - ratio), lo, hi)
}

// ----- Normal shock (state 2 downstream of state 1) -------------------------

/// Downstream Mach number `M2` across a normal shock.
pub fn mach_behind_shock(m1: f64, k: f64) -> Result<f64> {
    require_supersonic("normal shock", m1)?;
    require_k(k)?;
    let m1s = m1 * m1;
    Ok(libm::sqrt(
        ((k - 1.0) * m1s + 2.0) / (2.0 * k * m1s - (k - 1.0)),
    ))
}

/// Static pressure ratio `P2/P1` across a normal shock.
pub fn shock_pressure_ratio(m1: f64, k: f64) -> Result<f64> {
    require_supersonic("normal shock", m1)?;
    require_k(k)?;
    Ok((2.0 * k * m1 * m1 - (k - 1.0)) / (k + 1.0))
}

/// Static density ratio `rho2/rho1` across a normal shock.
pub fn shock_density_ratio(m1: f64, k: f64) -> Result<f64> {
    require_supersonic("normal shock", m1)?;
    require_k(k)?;
    let m1s = m1 * m1;
    Ok((k + 1.0) * m1s / (2.0 + (k - 1.0) * m1s))
}

/// Static temperature ratio `T2/T1` across a normal shock.
pub fn shock_temperature_ratio(m1: f64, k: f64) -> Result<f64> {
    require_supersonic("normal shock", m1)?;
    require_k(k)?;
    let m1s = m1 * m1;
    Ok((2.0 + (k - 1.0) * m1s) * (2.0 * k * m1s - (k - 1.0)) / ((k + 1.0) * (k + 1.0) * m1s))
}

/// Stagnation pressure ratio `P02/P01` across a normal shock (the loss).
pub fn shock_stagnation_pressure_ratio(m1: f64, k: f64) -> Result<f64> {
    require_supersonic("normal shock", m1)?;
    require_k(k)?;
    let m1s = m1 * m1;
    let a = (k + 1.0) * m1s / (2.0 + (k - 1.0) * m1s);
    let b = (k + 1.0) / (2.0 * k * m1s - (k - 1.0));
    Ok(libm::pow(a, k / (k - 1.0)) * libm::pow(b, 1.0 / (k - 1.0)))
}

// ----- Rayleigh flow (frictionless duct with heat addition) -----------------

/// Rayleigh `T0/T0*` (ratio to the sonic-reference stagnation temperature).
pub fn rayleigh_t0_over_t0star(m: f64, k: f64) -> Result<f64> {
    require_mach(m)?;
    require_k(k)?;
    let m2 = m * m;
    let denom = 1.0 + k * m2;
    Ok((k + 1.0) * m2 * (2.0 + (k - 1.0) * m2) / (denom * denom))
}

/// Rayleigh static temperature ratio `T/T*`.
pub fn rayleigh_t_over_tstar(m: f64, k: f64) -> Result<f64> {
    require_mach(m)?;
    require_k(k)?;
    let r = m * (1.0 + k) / (1.0 + k * m * m);
    Ok(r * r)
}

/// Rayleigh static pressure ratio `P/P*`.
pub fn rayleigh_p_over_pstar(m: f64, k: f64) -> Result<f64> {
    require_mach(m)?;
    require_k(k)?;
    Ok((1.0 + k) / (1.0 + k * m * m))
}

/// Rayleigh stagnation pressure ratio `P0/P0*`.
pub fn rayleigh_p0_over_p0star(m: f64, k: f64) -> Result<f64> {
    require_mach(m)?;
    require_k(k)?;
    let base = (2.0 + (k - 1.0) * m * m) / (k + 1.0);
    Ok(((1.0 + k) / (1.0 + k * m * m)) * libm::pow(base, k / (k - 1.0)))
}

// ----- Fanno flow (adiabatic duct with friction) ----------------------------

/// Fanno static temperature ratio `T/T*`.
pub fn fanno_t_over_tstar(m: f64, k: f64) -> Result<f64> {
    require_mach(m)?;
    require_k(k)?;
    Ok((k + 1.0) / (2.0 + (k - 1.0) * m * m))
}

/// Fanno static pressure ratio `P/P*`.
pub fn fanno_p_over_pstar(m: f64, k: f64) -> Result<f64> {
    require_mach(m)?;
    require_k(k)?;
    Ok((1.0 / m) * libm::sqrt((k + 1.0) / (2.0 + (k - 1.0) * m * m)))
}

/// Fanno stagnation pressure ratio `P0/P0*`.
pub fn fanno_p0_over_p0star(m: f64, k: f64) -> Result<f64> {
    require_mach(m)?;
    require_k(k)?;
    let base = (2.0 + (k - 1.0) * m * m) / (k + 1.0);
    Ok((1.0 / m) * libm::pow(base, (k + 1.0) / (2.0 * (k - 1.0))))
}

/// Fanno friction parameter `4 f Lmax / D` (Fanning friction factor `f`).
pub fn fanno_4f_lmax_over_d(m: f64, k: f64) -> Result<f64> {
    require_mach(m)?;
    require_k(k)?;
    let m2 = m * m;
    Ok((1.0 - m2) / (k * m2)
        + (k + 1.0) / (2.0 * k) * libm::log((k + 1.0) * m2 / (2.0 + (k - 1.0) * m2)))
}

// ----- Prandtl–Meyer expansion ----------------------------------------------

/// Prandtl–Meyer function `nu(M)` [rad], the turn angle from M = 1 to M.
pub fn prandtl_meyer(m: f64, k: f64) -> Result<f64> {
    require_supersonic("Prandtl-Meyer function", m)?;
    require_k(k)?;
    let t = libm::sqrt((k + 1.0) / (k - 1.0));
    let s = libm::sqrt(m * m - 1.0);
    Ok(t * libm::atan(s / t) - libm::atan(s))
}

/// Inverts the Prandtl–Meyer function: Mach number for a given `nu` [rad].
///
/// Valid on `[0, nu_max)` with `nu_max = π/2 · (√((k+1)/(k−1)) − 1)`, the
/// asymptotic turn angle for expansion into vacuum.
pub fn mach_from_prandtl_meyer(nu: f64, k: f64) -> Result<f64> {
    require_k(k)?;
    let nu_max = 0.5 * std::f64::consts::PI * (libm::sqrt((k + 1.0) / (k - 1.0)) - 1.0);
    if nu < 0.0 || nu >= nu_max {
        return Err(err(format!(
            "Compressible-flow: Prandtl-Meyer angle {nu:.4} rad is outside (0, {nu_max:.4}) \
             for k={k}."
        )));
    }
    if nu == 0.0 {
        return Ok(1.0);
    }
    bisect(|m| Ok(prandtl_meyer(m, k)? - nu), 1.0, 1e4)
}

// ----- Oblique shock (θ–β–M) ------------------------------------------------

/// Mach angle `mu = asin(1/M)` [rad].
pub fn mach_angle(m: f64) -> Result<f64> {
    require_supersonic("Mach angle", m)?;
    Ok(libm::asin(1.0 / m))
}

/// Flow-deflection angle `theta` [rad] for an oblique shock of wave angle
/// `beta` [rad] on upstream Mach `m1` (the θ–β–M relation).
pub fn theta_oblique(m1: f64, beta: f64, k: f64) -> Result<f64> {
    require_supersonic("oblique shock", m1)?;
    require_k(k)?;
    let m1n2 = m1 * m1 * libm::sin(beta) * libm::sin(beta);
    let num = 2.0 / libm::tan(beta) * (m1n2 - 1.0);
    let den = m1 * m1 * (k + libm::cos(2.0 * beta)) + 2.0;
    Ok(libm::atan(num / den))
}

/// Oblique-shock wave angle `beta` [rad] for a given deflection `theta` [rad].
///
/// `branch` is matched by prefix: `"weak"` (attached, smaller β) or `"strong"`
/// (larger β). θ(β) rises from 0 at the Mach angle to a maximum and falls back
/// to 0 at π/2; the peak is located by the Java's **fixed 400-interval scan**
/// (401 samples) and each monotone branch is then bisected. The scan resolution
/// is part of the answer — a finer grid moves both the detachment threshold and
/// the bracket, so it is transcribed exactly.
pub fn beta_oblique(m1: f64, theta: f64, k: f64, branch: &str) -> Result<f64> {
    require_supersonic("oblique shock", m1)?;
    require_k(k)?;
    if theta <= 0.0 {
        return Err(err(format!(
            "Compressible-flow: oblique-shock deflection theta must be > 0, got {theta}."
        )));
    }
    let b = branch.trim().to_lowercase();
    let weak = b.starts_with("weak");
    let strong = b.starts_with("strong");
    if !weak && !strong {
        return Err(err(format!(
            "Compressible-flow: beta_oblique branch must be 'weak' or 'strong', got '{branch}'."
        )));
    }
    let beta_min = libm::asin(1.0 / m1); // Mach wave (theta -> 0)
    let beta_max = 0.5 * std::f64::consts::PI; // normal shock (theta -> 0)
    let mut beta_peak = beta_min;
    let mut theta_peak = 0.0;
    let n = 400;
    for i in 0..=n {
        let beta = beta_min + (beta_max - beta_min) * i as f64 / n as f64;
        let th = theta_oblique(m1, beta, k)?;
        if th > theta_peak {
            theta_peak = th;
            beta_peak = beta;
        }
    }
    if theta > theta_peak {
        return Err(err(format!(
            "Compressible-flow: deflection theta={theta:.4} rad exceeds the maximum \
             {theta_peak:.4} rad for M1={m1}, k={k} (shock detaches)."
        )));
    }
    let residual = |beta: f64| Ok(theta_oblique(m1, beta, k)? - theta);
    if weak {
        bisect(residual, beta_min, beta_peak) // increasing branch
    } else {
        bisect(residual, beta_peak, beta_max) // decreasing branch
    }
}

// ----- shared numerics ------------------------------------------------------

/// Bisection root-finder for a continuous `f` sign-bracketed by `[lo, hi]`;
/// the sign bookkeeping serves both monotone directions.
///
/// Transcribed from `CompressibleFlow.bisect`: at most **200** halvings, and
/// the loop returns the midpoint as soon as `|f(mid)| < 1e-12` **or** the
/// bracket is narrower than `1e-12`. Note the Java's asymmetry — `flo` is
/// refreshed when the low end moves, `fhi` never is; it is only read for the
/// initial bracket check. That is transcribed, not fixed, because the
/// midpoint sequence (and therefore the last digits) depends on it.
fn bisect(f: impl Fn(f64) -> Result<f64>, lo: f64, hi: f64) -> Result<f64> {
    let mut lo = lo;
    let mut hi = hi;
    let mut flo = f(lo)?;
    let fhi = f(hi)?;
    if flo == 0.0 {
        return Ok(lo);
    }
    if fhi == 0.0 {
        return Ok(hi);
    }
    if flo * fhi > 0.0 {
        return Err(err(
            "Compressible-flow: target is outside the solvable range for the requested branch.",
        ));
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        let fm = f(mid)?;
        if libm::fabs(fm) < 1e-12 || (hi - lo) < 1e-12 {
            return Ok(mid);
        }
        if (fm > 0.0) == (flo > 0.0) {
            lo = mid;
            flo = fm;
        } else {
            hi = mid;
        }
    }
    Ok(0.5 * (lo + hi))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Air and a monatomic gas — the two `k` values the fixtures pin.
    const K_AIR: f64 = 1.4;
    const K_MONO: f64 = 1.6666666666666667;

    /// The golden corpus is generated by the Java oracle and compared at
    /// `1e-9` relative (`fixtures/README.md`); these assertions are tighter.
    #[track_caller]
    fn oracle(actual: f64, expected: f64) {
        let diff = (actual - expected).abs();
        assert!(
            diff <= 1e-12 || diff <= 1e-13 * expected.abs().max(actual.abs()),
            "expected {expected}, got {actual} (diff {diff})"
        );
    }

    // --- isentropic, vs fixtures/golden/compressible-isentropic.json --------

    #[test]
    fn isentropic_ratios_match_the_oracle() {
        oracle(t0_over_t(0.5, K_AIR).unwrap(), 1.05);
        oracle(p0_over_p(0.5, K_AIR).unwrap(), 1.1862126380443982);
        oracle(rho0_over_rho(0.5, K_AIR).unwrap(), 1.129726321947046);
        oracle(a_over_astar(0.5, K_AIR).unwrap(), 1.3398437500000004);

        oracle(t0_over_t(2.5, K_AIR).unwrap(), 2.25);
        oracle(p0_over_p(2.5, K_AIR).unwrap(), 17.085937500000007);
        oracle(rho0_over_rho(2.5, K_AIR).unwrap(), 7.593750000000003);
        oracle(a_over_astar(2.5, K_AIR).unwrap(), 2.636718750000001);

        oracle(t0_over_t(1.2, K_MONO).unwrap(), 1.48);
        oracle(p0_over_p(1.2, K_MONO).unwrap(), 2.664736609273044);
        oracle(a_over_astar(0.3, K_MONO).unwrap(), 1.9891874999999992);
    }

    #[test]
    fn area_ratio_inverse_picks_the_requested_branch() {
        oracle(
            mach_from_a_over_astar(1.3398437500000004, K_AIR, "subsonic").unwrap(),
            0.5000000000003306,
        );
        oracle(
            mach_from_a_over_astar(2.636718750000001, K_AIR, "supersonic").unwrap(),
            2.499999999999659,
        );
        // The abbreviated spellings the Java accepts by prefix, and the sonic
        // shortcut that returns exactly 1 without iterating.
        oracle(mach_from_a_over_astar(1.0, K_AIR, "sup").unwrap(), 1.0);
        oracle(mach_from_a_over_astar(1.0, K_AIR, " SUB ").unwrap(), 1.0);
        // The cd-nozzle-shock fixture's upstream Mach number.
        oracle(
            mach_from_a_over_astar(2.0, K_AIR, "supersonic").unwrap(),
            2.1971981216524625,
        );
    }

    // --- normal shock, vs fixtures/golden/compressible-normal-shock.json ----

    #[test]
    fn normal_shock_ratios_match_the_oracle() {
        oracle(mach_behind_shock(3.0, K_AIR).unwrap(), 0.4751909633114914);
        oracle(
            shock_pressure_ratio(3.0, K_AIR).unwrap(),
            10.333333333333332,
        );
        oracle(
            shock_temperature_ratio(3.0, K_AIR).unwrap(),
            2.6790123456790123,
        );
        oracle(shock_density_ratio(3.0, K_AIR).unwrap(), 3.857142857142857);
        oracle(
            shock_stagnation_pressure_ratio(3.0, K_AIR).unwrap(),
            0.32834388819073684,
        );

        oracle(mach_behind_shock(4.5, K_MONO).unwrap(), 0.4815809376431411);
        oracle(
            shock_pressure_ratio(4.5, K_MONO).unwrap(),
            25.062499999999996,
        );
        oracle(
            shock_temperature_ratio(4.5, K_MONO).unwrap(),
            7.19386574074074,
        );
        oracle(
            shock_density_ratio(4.5, K_MONO).unwrap(),
            3.4838709677419355,
        );
        oracle(
            shock_stagnation_pressure_ratio(4.5, K_MONO).unwrap(),
            0.18055876235036986,
        );
    }

    #[test]
    fn a_sonic_shock_is_the_identity() {
        oracle(mach_behind_shock(1.0, K_AIR).unwrap(), 1.0);
        oracle(shock_pressure_ratio(1.0, K_AIR).unwrap(), 1.0);
        oracle(shock_temperature_ratio(1.0, K_AIR).unwrap(), 1.0);
        oracle(shock_density_ratio(1.0, K_AIR).unwrap(), 1.0);
        oracle(shock_stagnation_pressure_ratio(1.0, K_AIR).unwrap(), 1.0);
    }

    // --- Rayleigh / Fanno, vs compressible-rayleigh-fanno.json --------------

    #[test]
    fn rayleigh_ratios_match_the_oracle() {
        oracle(
            rayleigh_t0_over_t0star(0.4, K_AIR).unwrap(),
            0.5290272971933873,
        );
        oracle(
            rayleigh_t_over_tstar(0.4, K_AIR).unwrap(),
            0.6151480199923106,
        );
        oracle(
            rayleigh_p_over_pstar(0.4, K_AIR).unwrap(),
            1.9607843137254901,
        );
        oracle(
            rayleigh_p0_over_p0star(0.4, K_AIR).unwrap(),
            1.1565766050531407,
        );

        oracle(
            rayleigh_t0_over_t0star(2.2, K_AIR).unwrap(),
            0.7561347355586039,
        );
        oracle(
            rayleigh_t_over_tstar(2.2, K_AIR).unwrap(),
            0.46105776558451456,
        );
        oracle(
            rayleigh_p_over_pstar(2.2, K_AIR).unwrap(),
            0.30864197530864196,
        );
        oracle(
            rayleigh_p0_over_p0star(2.2, K_AIR).unwrap(),
            1.7434458294048771,
        );

        oracle(rayleigh_t0_over_t0star(1.0, K_AIR).unwrap(), 1.0);
        oracle(
            rayleigh_p0_over_p0star(3.1, K_MONO).unwrap(),
            2.7652345766157995,
        );
    }

    #[test]
    fn fanno_ratios_match_the_oracle() {
        oracle(fanno_t_over_tstar(0.4, K_AIR).unwrap(), 1.1627906976744184);
        oracle(fanno_p_over_pstar(0.4, K_AIR).unwrap(), 2.69581933008596);
        oracle(
            fanno_p0_over_p0star(0.4, K_AIR).unwrap(),
            1.5901400000000003,
        );
        oracle(
            fanno_4f_lmax_over_d(0.4, K_AIR).unwrap(),
            2.3084926508453765,
        );

        oracle(fanno_t_over_tstar(2.2, K_AIR).unwrap(), 0.6097560975609756);
        oracle(fanno_p_over_pstar(2.2, K_AIR).unwrap(), 0.35494036792865014);
        oracle(
            fanno_p0_over_p0star(2.2, K_AIR).unwrap(),
            2.0049745454545462,
        );
        oracle(
            fanno_4f_lmax_over_d(2.2, K_AIR).unwrap(),
            0.3609098177991814,
        );

        // The sonic reference: the duct length to choking vanishes at M = 1.
        oracle(fanno_4f_lmax_over_d(1.0, K_AIR).unwrap(), 0.0);
        oracle(
            fanno_4f_lmax_over_d(0.25, K_MONO).unwrap(),
            6.9955792504074115,
        );
    }

    // --- Prandtl–Meyer / oblique, vs compressible-oblique-expansion.json ----

    #[test]
    fn prandtl_meyer_and_its_inverse_match_the_oracle() {
        oracle(prandtl_meyer(2.6, K_AIR).unwrap(), 0.722823009502086);
        oracle(
            mach_from_prandtl_meyer(0.722823009502086, K_AIR).unwrap(),
            2.60000000000045,
        );
        oracle(mach_from_prandtl_meyer(0.0, K_AIR).unwrap(), 1.0);
        oracle(prandtl_meyer(3.4, K_MONO).unwrap(), 0.7659129021731661);
        oracle(
            mach_from_prandtl_meyer(0.7659129021731661, K_MONO).unwrap(),
            3.3999999999962345,
        );
    }

    #[test]
    fn mach_angle_matches_the_oracle() {
        oracle(mach_angle(2.6).unwrap(), 0.3947911196997615);
        // The oracle's 1.5707963267948966 *is* FRAC_PI_2 bit for bit — a sonic
        // Mach wave is normal to the flow.
        oracle(mach_angle(1.0).unwrap(), std::f64::consts::FRAC_PI_2);
    }

    #[test]
    fn oblique_shock_has_a_weak_and_a_strong_root() {
        let theta = 20.0 * std::f64::consts::PI / 180.0;
        oracle(theta, 0.3490658503988659);
        let weak = beta_oblique(2.6, theta, K_AIR, "weak").unwrap();
        let strong = beta_oblique(2.6, theta, K_AIR, "strong").unwrap();
        oracle(weak, 0.7264279972824406);
        oracle(strong, 1.4071758877295204);
        // Both roots reproduce the requested deflection to the bisection's
        // tolerance, on their own side of the peak.
        oracle(
            theta_oblique(2.6, weak, K_AIR).unwrap(),
            0.34906585039876925,
        );
        oracle(
            theta_oblique(2.6, strong, K_AIR).unwrap(),
            0.34906585039933663,
        );
        oracle(
            beta_oblique(3.4, 0.35, K_MONO, "weak").unwrap(),
            0.6699558813315369,
        );
    }

    #[test]
    fn oblique_shock_composes_with_the_normal_shock_relations() {
        // The classic wedge calculation: resolve M1 normal to the wave, cross
        // the shock, then unresolve through (beta − theta).
        let theta = 20.0 * std::f64::consts::PI / 180.0;
        let weak = beta_oblique(2.6, theta, K_AIR, "weak").unwrap();
        let m2n = mach_behind_shock(2.6 * libm::sin(weak), K_AIR).unwrap();
        oracle(m2n, 0.6337225562379606);
        oracle(m2n / libm::sin(weak - theta), 1.7198779132133066);
    }

    // --- domain guards ------------------------------------------------------

    #[test]
    fn k_must_exceed_one() {
        for k in [1.0, 0.9, -1.0, f64::NAN] {
            let e = t0_over_t(2.0, k).unwrap_err().to_string();
            assert!(e.contains("ratio of specific heats k must be > 1"), "{e}");
        }
    }

    #[test]
    fn mach_must_be_positive() {
        for m in [0.0, -0.5, f64::NAN] {
            let e = t0_over_t(m, K_AIR).unwrap_err().to_string();
            assert!(e.contains("Mach number must be > 0"), "{e}");
        }
        // The derived ratios inherit the guard through t0_over_t.
        assert!(p0_over_p(0.0, K_AIR).is_err());
        assert!(rho0_over_rho(0.0, K_AIR).is_err());
        assert!(a_over_astar(0.0, K_AIR).is_err());
    }

    #[test]
    fn shock_and_expansion_relations_require_supersonic_flow() {
        let e = mach_behind_shock(0.8, K_AIR).unwrap_err().to_string();
        assert!(e.contains("normal shock requires a supersonic"), "{e}");
        let e = prandtl_meyer(0.8, K_AIR).unwrap_err().to_string();
        assert!(
            e.contains("Prandtl-Meyer function requires a supersonic"),
            "{e}"
        );
        let e = mach_angle(0.8).unwrap_err().to_string();
        assert!(e.contains("Mach angle requires a supersonic"), "{e}");
        let e = theta_oblique(0.8, 0.5, K_AIR).unwrap_err().to_string();
        assert!(e.contains("oblique shock requires a supersonic"), "{e}");
        assert!(shock_pressure_ratio(0.8, K_AIR).is_err());
        assert!(shock_density_ratio(0.8, K_AIR).is_err());
        assert!(shock_temperature_ratio(0.8, K_AIR).is_err());
        assert!(shock_stagnation_pressure_ratio(0.8, K_AIR).is_err());
    }

    #[test]
    fn area_ratio_inverse_rejects_bad_input() {
        let e = mach_from_a_over_astar(0.5, K_AIR, "subsonic")
            .unwrap_err()
            .to_string();
        assert!(e.contains("A/A* must be >= 1"), "{e}");
        let e = mach_from_a_over_astar(2.0, K_AIR, "transonic")
            .unwrap_err()
            .to_string();
        assert!(e.contains("must be 'subsonic' or 'supersonic'"), "{e}");
        // The supersonic bracket stops at M = 50, where A/A* ~ 1.46e6. A larger
        // target is unreachable and bisection reports it rather than returning
        // a wrong root — the Java bracket is a hard ceiling, transcribed.
        assert!(mach_from_a_over_astar(1e6, K_AIR, "supersonic").is_ok());
        let e = mach_from_a_over_astar(1e7, K_AIR, "supersonic")
            .unwrap_err()
            .to_string();
        assert!(e.contains("outside the solvable range"), "{e}");
    }

    #[test]
    fn prandtl_meyer_inverse_rejects_angles_outside_the_vacuum_limit() {
        // Spelled exactly as the function does: `(k+1)/(k-1)` for k = 1.4 is
        // 6.000000000000003, not the 5.999999999999999 that `2.4/0.4` gives,
        // and the two differ in the last digit of nu_max.
        let nu_max = 0.5 * std::f64::consts::PI * (libm::sqrt((K_AIR + 1.0) / (K_AIR - 1.0)) - 1.0);
        let e = mach_from_prandtl_meyer(nu_max, K_AIR)
            .unwrap_err()
            .to_string();
        assert!(e.contains("is outside (0,"), "{e}");
        assert!(mach_from_prandtl_meyer(-0.1, K_AIR).is_err());
        // Inside the limit and inside the bracket, it solves.
        assert!(mach_from_prandtl_meyer(nu_max - 0.01, K_AIR).is_ok());
        // But the range check admits angles the bracket cannot reach: nu is
        // only nu_max − 5/M asymptotically, so the Java's `[1, 1e4]` bracket
        // tops out ~5e-4 rad short of nu_max and anything closer fails in
        // `bisect` instead. Transcribed, not smoothed over.
        let e = mach_from_prandtl_meyer(nu_max - 1e-6, K_AIR)
            .unwrap_err()
            .to_string();
        assert!(e.contains("outside the solvable range"), "{e}");
    }

    #[test]
    fn oblique_shock_reports_detachment_and_bad_branches() {
        let e = beta_oblique(1.5, 0.6, K_AIR, "weak")
            .unwrap_err()
            .to_string();
        assert!(e.contains("exceeds the maximum"), "{e}");
        assert!(e.contains("shock detaches"), "{e}");
        let e = beta_oblique(2.6, 0.3, K_AIR, "mild")
            .unwrap_err()
            .to_string();
        assert!(e.contains("must be 'weak' or 'strong'"), "{e}");
        let e = beta_oblique(2.6, 0.0, K_AIR, "weak")
            .unwrap_err()
            .to_string();
        assert!(e.contains("deflection theta must be > 0"), "{e}");
    }

    #[test]
    fn errors_are_property_evaluation_failures() {
        // The Java raises PropertyEvaluationException, which the parity gate
        // maps to FreesError::Property (tests/parity.rs `error_matches`).
        assert!(matches!(
            t0_over_t(2.0, 1.0),
            Err(FreesError::Property { .. })
        ));
    }
}
