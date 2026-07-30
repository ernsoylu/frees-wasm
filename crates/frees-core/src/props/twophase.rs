//! Two-phase (gas–liquid) flow constitutive functions.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/props/TwoPhase.java`
//! (169 LOC), in full: the Lockhart–Martinelli / Chisholm frictional
//! multiplier and its turbulent–turbulent parameter, three void-fraction
//! models (homogeneous, Zivi, Rouhani–Axelsson), the Friedel liquid-only
//! multiplier and the separated-flow momentum flux.
//!
//! These turn a single-phase frictional drop into a two-phase one, so a
//! refrigerant or steam line with a finite quality closes a realistic `ΔP`
//! equation instead of the homogeneous Darcy approximation.
//!
//! Everything is SI and every result is dimensionless except
//! [`momentum_flux`], which is a pressure [Pa].
//!
//! # Clamping is deliberate
//!
//! The void fractions return `0` below the dome and `1` above it, and
//! [`friedel_phi2`] / [`momentum_flux`] clip their quality and void fraction
//! into the open interval, so a Newton iterate that strays outside the dome
//! gets a finite value and a usable derivative instead of a division by zero.
//! That is the Java's behaviour and it is load-bearing for step-halving.

use crate::diag::{FreesError, Result};

/// Standard gravity [m/s²] — the Java's own literal, local to this file.
const GRAVITY: f64 = 9.80665;

fn err(message: impl Into<String>) -> FreesError {
    FreesError::property(message)
}

fn require_positive(rho_l: f64, rho_g: f64) -> Result<()> {
    if rho_l <= 0.0 || rho_g <= 0.0 {
        return Err(err("two-phase: densities must be > 0."));
    }
    Ok(())
}

/// Chisholm two-phase frictional multiplier on the liquid-alone drop,
/// `φ_l² = 1 + C/X + 1/X²`.
///
/// `x` is the Martinelli parameter and `c` the Chisholm constant: 20
/// turbulent–turbulent, 12 laminar–turbulent, 10 turbulent–laminar, 5
/// laminar–laminar. The two-phase drop is `ΔP_tp = φ_l² · ΔP_liquid-alone`.
pub fn lm_phi2(x: f64, c: f64) -> Result<f64> {
    if x <= 0.0 {
        return Err(err("lm_phi2: Martinelli parameter X must be > 0."));
    }
    Ok(1.0 + c / x + 1.0 / (x * x))
}

/// Turbulent–turbulent Martinelli parameter
/// `X_tt = ((1−x)/x)^0.9 · (ρ_g/ρ_l)^0.5 · (μ_l/μ_g)^0.1`
/// from the vapour quality `quality` ∈ (0, 1) and the phase densities and
/// viscosities.
pub fn lm_martinelli_tt(quality: f64, rho_l: f64, rho_g: f64, mu_l: f64, mu_g: f64) -> Result<f64> {
    if quality <= 0.0 || quality >= 1.0 {
        return Err(err("lm_martinelli_tt: quality x must be in (0, 1)."));
    }
    if rho_l <= 0.0 || rho_g <= 0.0 || mu_l <= 0.0 || mu_g <= 0.0 {
        return Err(err(
            "lm_martinelli_tt: densities and viscosities must be > 0.",
        ));
    }
    Ok(libm::pow((1.0 - quality) / quality, 0.9)
        * libm::pow(rho_g / rho_l, 0.5)
        * libm::pow(mu_l / mu_g, 0.1))
}

/// Homogeneous (no-slip) void fraction `α = 1 / (1 + ((1−x)/x)·(ρ_g/ρ_l))`,
/// clamped to `[0, 1]` outside the dome (`x ≤ 0 → 0`, `x ≥ 1 → 1`).
pub fn void_homogeneous(x: f64, rho_l: f64, rho_g: f64) -> Result<f64> {
    if x <= 0.0 {
        return Ok(0.0);
    }
    if x >= 1.0 {
        return Ok(1.0);
    }
    require_positive(rho_l, rho_g)?;
    Ok(1.0 / (1.0 + ((1.0 - x) / x) * (rho_g / rho_l)))
}

/// Zivi void fraction — the homogeneous form with a slip ratio
/// `S = (ρ_l/ρ_g)^(1/3)` (minimum entropy production). Clamped to `[0, 1]`.
pub fn void_zivi(x: f64, rho_l: f64, rho_g: f64) -> Result<f64> {
    if x <= 0.0 {
        return Ok(0.0);
    }
    if x >= 1.0 {
        return Ok(1.0);
    }
    require_positive(rho_l, rho_g)?;
    let s = libm::cbrt(rho_l / rho_g);
    Ok(1.0 / (1.0 + ((1.0 - x) / x) * (rho_g / rho_l) * s))
}

/// Rouhani–Axelsson drift-flux void fraction (the orientation-aware default,
/// vertical / co-current form):
///
/// ```text
///   a  = (x/rho_g) / [ C0*(x/rho_g + (1-x)/rho_l) + u_gu/G ]
///   C0 = 1 + 0.12*(1-x)
///   u_gu = 1.18*(1-x)*[g*sigma*(rho_l-rho_g)/rho_l^2]^0.25
/// ```
///
/// with mass flux `g` [kg/m²s] and surface tension `sigma` [N/m]. Clamped to
/// `[0, 1]`.
pub fn void_rouhani(x: f64, rho_l: f64, rho_g: f64, g: f64, sigma: f64) -> Result<f64> {
    if x <= 0.0 {
        return Ok(0.0);
    }
    if x >= 1.0 {
        return Ok(1.0);
    }
    require_positive(rho_l, rho_g)?;
    if g <= 0.0 || sigma <= 0.0 || rho_l <= rho_g {
        return Err(err(
            "void_rouhani: mass flux G>0, surface tension σ>0 and ρ_l>ρ_g required.",
        ));
    }
    let c0 = 1.0 + 0.12 * (1.0 - x);
    let ugu =
        1.18 * (1.0 - x) * libm::pow(GRAVITY * sigma * (rho_l - rho_g) / (rho_l * rho_l), 0.25);
    let denom = c0 * (x / rho_g + (1.0 - x) / rho_l) + ugu / g;
    let alpha = (x / rho_g) / denom;
    Ok(java_max(0.0, java_min(1.0, alpha)))
}

/// Friedel two-phase frictional multiplier `φ_lo²` on the *liquid-only*
/// pressure drop:
///
/// ```text
///   phi_lo^2 = E + 3.24*F*H / (Fr^0.045 * We^0.035)
/// ```
///
/// with `E`, `F`, `H` the property/quality groups, the homogeneous Froude
/// `Fr = G²/(g·D·ρ_h²)` and Weber `We = G²·D/(ρ_h·σ)` numbers, and
/// liquid-/gas-only Fanning friction factors from a Blasius law. `g` is the
/// mass flux [kg/m²s], `d` the diameter [m], `sigma` the surface tension
/// [N/m]. The quality is clipped into `(0, 1)`.
#[allow(clippy::too_many_arguments)]
pub fn friedel_phi2(
    x: f64,
    rho_l: f64,
    rho_g: f64,
    mu_l: f64,
    mu_g: f64,
    g: f64,
    d: f64,
    sigma: f64,
) -> Result<f64> {
    require_positive(rho_l, rho_g)?;
    if mu_l <= 0.0 || mu_g <= 0.0 || g <= 0.0 || d <= 0.0 || sigma <= 0.0 {
        return Err(err("friedel_phi2: viscosities, G, D and σ must be > 0."));
    }
    let xx = java_max(1e-6, java_min(1.0 - 1e-6, x));
    let rho_h = 1.0 / (xx / rho_g + (1.0 - xx) / rho_l);
    let f_lo = blasius_fanning(g * d / mu_l);
    let f_go = blasius_fanning(g * d / mu_g);
    let e = (1.0 - xx) * (1.0 - xx) + xx * xx * (rho_l * f_go) / (rho_g * f_lo);
    let f = libm::pow(xx, 0.78) * libm::pow(1.0 - xx, 0.224);
    let h = libm::pow(rho_l / rho_g, 0.91)
        * libm::pow(mu_g / mu_l, 0.19)
        * libm::pow(1.0 - mu_g / mu_l, 0.7);
    let fr = g * g / (GRAVITY * d * rho_h * rho_h);
    let we = g * g * d / (rho_h * sigma);
    Ok(e + 3.24 * f * h / (libm::pow(fr, 0.045) * libm::pow(we, 0.035)))
}

/// Separated-flow momentum flux `G²·[x²/(ρ_g·α) + (1−x)²/(ρ_l·(1−α))]` [Pa] at
/// one station. The acceleration pressure drop across an element is the
/// difference of this term between outlet and inlet. `alpha` is clipped into
/// `(0, 1)` with a `1e-9` margin.
pub fn momentum_flux(x: f64, rho_l: f64, rho_g: f64, alpha: f64, g: f64) -> Result<f64> {
    require_positive(rho_l, rho_g)?;
    let a = java_max(1e-9, java_min(1.0 - 1e-9, alpha));
    Ok(g * g * (x * x / (rho_g * a) + (1.0 - x) * (1.0 - x) / (rho_l * (1.0 - a))))
}

/// Blasius Fanning friction factor `0.079·Re^-0.25`, with a laminar floor
/// `16/Re` below Re = 1187 (roughly where the two laws cross).
fn blasius_fanning(re: f64) -> f64 {
    let r = java_max(re, 1.0);
    if r < 1187.0 {
        16.0 / r
    } else {
        0.079 * libm::pow(r, -0.25)
    }
}

/// `Math.min`: NaN-propagating, `-0.0 < 0.0`.
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

/// `Math.max`: NaN-propagating, `0.0 > -0.0`.
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

#[cfg(test)]
mod tests {
    use super::*;

    // Saturated R134a-like properties — the state the fixtures pin.
    const RHO_L: f64 = 1187.5;
    const RHO_G: f64 = 25.75;
    const MU_L: f64 = 0.000185;
    const MU_G: f64 = 0.0000117;
    const SIGMA: f64 = 0.0082;
    const MASS_FLUX: f64 = 300.0;
    const DIAM: f64 = 0.008;

    #[track_caller]
    fn oracle(actual: f64, expected: f64) {
        let diff = (actual - expected).abs();
        assert!(
            diff <= 1e-12 || diff <= 1e-13 * expected.abs().max(actual.abs()),
            "expected {expected}, got {actual} (diff {diff})"
        );
    }

    // --- vs fixtures/golden/twophase-lockhart-martinelli.json ---------------

    #[test]
    fn martinelli_parameter_matches_the_oracle() {
        oracle(
            lm_martinelli_tt(0.35, RHO_L, RHO_G, MU_L, MU_G).unwrap(),
            0.3387904621342631,
        );
        oracle(
            lm_martinelli_tt(0.02, RHO_L, RHO_G, MU_L, MU_G).unwrap(),
            6.443871423588654,
        );
        oracle(
            lm_martinelli_tt(0.95, RHO_L, RHO_G, MU_L, MU_G).unwrap(),
            0.013711726857004686,
        );
    }

    #[test]
    fn chisholm_multiplier_matches_the_oracle_for_all_four_constants() {
        let xtt = 0.3387904621342631;
        oracle(lm_phi2(xtt, 20.0).unwrap(), 68.74593597301762);
        oracle(lm_phi2(xtt, 12.0).unwrap(), 45.132520324882904);
        oracle(lm_phi2(xtt, 10.0).unwrap(), 39.229166412849224);
        oracle(lm_phi2(xtt, 5.0).unwrap(), 24.470781632765032);
        oracle(lm_phi2(6.443871423588654, 20.0).unwrap(), 4.1278070103493);
        oracle(
            lm_phi2(0.013711726857004686, 20.0).unwrap(),
            6778.43038569912,
        );
        // X = 1 collapses to 1 + C + 1.
        oracle(lm_phi2(1.0, 20.0).unwrap(), 22.0);
    }

    // --- vs fixtures/golden/twophase-void-friedel.json ----------------------

    #[test]
    fn void_fractions_match_the_oracle() {
        oracle(
            void_homogeneous(0.35, RHO_L, RHO_G).unwrap(),
            0.9612882708375495,
        );
        oracle(void_zivi(0.35, RHO_L, RHO_G).unwrap(), 0.8738100526784753);
        oracle(
            void_rouhani(0.35, RHO_L, RHO_G, MASS_FLUX, SIGMA).unwrap(),
            0.8784400199754803,
        );
        // Slip always lowers the void fraction relative to no-slip.
        assert!(
            void_zivi(0.35, RHO_L, RHO_G).unwrap() < void_homogeneous(0.35, RHO_L, RHO_G).unwrap()
        );
    }

    #[test]
    fn void_fractions_clamp_outside_the_dome() {
        for x in [0.0, -0.2, -1e300] {
            oracle(void_homogeneous(x, RHO_L, RHO_G).unwrap(), 0.0);
            oracle(void_zivi(x, RHO_L, RHO_G).unwrap(), 0.0);
            oracle(
                void_rouhani(x, RHO_L, RHO_G, MASS_FLUX, SIGMA).unwrap(),
                0.0,
            );
        }
        for x in [1.0, 1.4, 1e300] {
            oracle(void_homogeneous(x, RHO_L, RHO_G).unwrap(), 1.0);
            oracle(void_zivi(x, RHO_L, RHO_G).unwrap(), 1.0);
            oracle(
                void_rouhani(x, RHO_L, RHO_G, MASS_FLUX, SIGMA).unwrap(),
                1.0,
            );
        }
        // The clamp runs *before* the density guard, so a nonsense density is
        // never reached outside the dome — Java's ordering, transcribed.
        assert!(void_homogeneous(0.0, -1.0, -1.0).is_ok());
        assert!(void_rouhani(1.0, -1.0, -1.0, -1.0, -1.0).is_ok());
    }

    #[test]
    fn friedel_multiplier_matches_the_oracle() {
        oracle(
            friedel_phi2(0.35, RHO_L, RHO_G, MU_L, MU_G, MASS_FLUX, DIAM, SIGMA).unwrap(),
            17.875255673715316,
        );
        oracle(
            friedel_phi2(0.02, RHO_L, RHO_G, MU_L, MU_G, MASS_FLUX, DIAM, SIGMA).unwrap(),
            3.2266721680149555,
        );
        // Quality clipped to 1e-6 / 1−1e-6 rather than dividing by zero.
        oracle(
            friedel_phi2(0.0, RHO_L, RHO_G, MU_L, MU_G, MASS_FLUX, DIAM, SIGMA).unwrap(),
            1.0010834401193758,
        );
        oracle(
            friedel_phi2(1.0, RHO_L, RHO_G, MU_L, MU_G, MASS_FLUX, DIAM, SIGMA).unwrap(),
            24.584012898697207,
        );
        // G = 4 kg/m²s puts both Blasius factors on the laminar 16/Re floor.
        oracle(
            friedel_phi2(0.35, RHO_L, RHO_G, MU_L, MU_G, 4.0, DIAM, SIGMA).unwrap(),
            30.260583775688982,
        );
    }

    #[test]
    fn momentum_flux_matches_the_oracle_and_clips_the_void_fraction() {
        oracle(
            momentum_flux(0.35, RHO_L, RHO_G, 0.8784400199754803, MASS_FLUX).unwrap(),
            750.8219012619154,
        );
        oracle(
            momentum_flux(0.35, RHO_L, RHO_G, 1.0, MASS_FLUX).unwrap(),
            32021053965.351536,
        );
        oracle(
            momentum_flux(0.35, RHO_L, RHO_G, 0.0, MASS_FLUX).unwrap(),
            428155339837.8462,
        );
    }

    // --- domain guards ------------------------------------------------------

    #[test]
    fn martinelli_parameter_must_be_positive() {
        for x in [0.0, -1.0] {
            let e = lm_phi2(x, 20.0).unwrap_err().to_string();
            assert!(e.contains("Martinelli parameter X must be > 0"), "{e}");
        }
    }

    #[test]
    fn martinelli_tt_rejects_saturated_ends_and_bad_properties() {
        for q in [0.0, 1.0, -0.1, 1.1] {
            let e = lm_martinelli_tt(q, RHO_L, RHO_G, MU_L, MU_G)
                .unwrap_err()
                .to_string();
            assert!(e.contains("quality x must be in (0, 1)"), "{e}");
        }
        let e = lm_martinelli_tt(0.5, 0.0, RHO_G, MU_L, MU_G)
            .unwrap_err()
            .to_string();
        assert!(e.contains("densities and viscosities must be > 0"), "{e}");
        assert!(lm_martinelli_tt(0.5, RHO_L, 0.0, MU_L, MU_G).is_err());
        assert!(lm_martinelli_tt(0.5, RHO_L, RHO_G, 0.0, MU_G).is_err());
        assert!(lm_martinelli_tt(0.5, RHO_L, RHO_G, MU_L, 0.0).is_err());
    }

    #[test]
    fn void_and_flux_reject_non_physical_densities_inside_the_dome() {
        let e = void_homogeneous(0.5, 0.0, RHO_G).unwrap_err().to_string();
        assert!(e.contains("two-phase: densities must be > 0"), "{e}");
        assert!(void_zivi(0.5, RHO_L, 0.0).is_err());
        assert!(momentum_flux(0.5, -1.0, RHO_G, 0.5, 1.0).is_err());
        assert!(friedel_phi2(0.5, -1.0, RHO_G, MU_L, MU_G, 1.0, 1.0, 1.0).is_err());
    }

    #[test]
    fn rouhani_requires_a_real_drift_flux_state() {
        let e = void_rouhani(0.5, RHO_L, RHO_G, 0.0, SIGMA)
            .unwrap_err()
            .to_string();
        assert!(e.contains("mass flux G>0"), "{e}");
        assert!(void_rouhani(0.5, RHO_L, RHO_G, MASS_FLUX, 0.0).is_err());
        // rho_l <= rho_g makes the drift velocity meaningless.
        assert!(void_rouhani(0.5, 25.75, 25.75, MASS_FLUX, SIGMA).is_err());
    }

    #[test]
    fn friedel_requires_positive_viscosities_flux_and_geometry() {
        let e = friedel_phi2(0.5, RHO_L, RHO_G, 0.0, MU_G, MASS_FLUX, DIAM, SIGMA)
            .unwrap_err()
            .to_string();
        assert!(e.contains("viscosities, G, D and"), "{e}");
        assert!(friedel_phi2(0.5, RHO_L, RHO_G, MU_L, 0.0, MASS_FLUX, DIAM, SIGMA).is_err());
        assert!(friedel_phi2(0.5, RHO_L, RHO_G, MU_L, MU_G, 0.0, DIAM, SIGMA).is_err());
        assert!(friedel_phi2(0.5, RHO_L, RHO_G, MU_L, MU_G, MASS_FLUX, 0.0, SIGMA).is_err());
        assert!(friedel_phi2(0.5, RHO_L, RHO_G, MU_L, MU_G, MASS_FLUX, DIAM, 0.0).is_err());
    }

    #[test]
    fn errors_are_property_evaluation_failures() {
        assert!(matches!(
            lm_phi2(0.0, 20.0),
            Err(FreesError::Property { .. })
        ));
    }
}
