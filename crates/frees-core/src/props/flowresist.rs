//! Flow-resistance constitutive functions for hydraulic / duct networks.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/props/FlowResistance.java`
//! (77 LOC), in full: the Reynolds number, the Darcy friction factor
//! (laminar / blended transition / Colebrook–White turbulent) and minor
//! (fitting) losses.
//!
//! These let pipe and duct components close a pressure-drop equation, so a
//! pump–pipe or fan–duct network has a well-posed operating point for the
//! Newton solver.
//!
//! All SI: Reynolds number and friction factor are dimensionless, minor loss is
//! a pressure [Pa].
//!
//! # Why nothing here throws on a bad flow state
//!
//! A Newton iterate routinely passes through zero and negative flow. [`reynolds`]
//! takes `|V|`, and [`friction_factor`] clamps a non-positive Reynolds number to
//! `1e-6` — giving a large but finite friction factor (high resistance at
//! vanishing flow) — so the solver steps through those states instead of
//! crashing. The 2300–4000 transition band is **linearly blended** for the same
//! reason: `f` and its numerical derivative stay continuous, which step-halving
//! depends on.

use crate::diag::{FreesError, Result};

/// Reynolds number `Re = ρ·|V|·D/μ` (dimensionless; the absolute value makes
/// reversed flow harmless).
pub fn reynolds(rho: f64, velocity: f64, diameter: f64, viscosity: f64) -> Result<f64> {
    if viscosity <= 0.0 {
        return Err(FreesError::property(
            "reynolds: dynamic viscosity must be > 0.",
        ));
    }
    // A zero/transient-negative flow gives Re = 0 (handled by frictionFactor);
    // it must not throw, so a Newton iterate passing through zero flow survives.
    Ok(rho * libm::fabs(velocity) * diameter / viscosity)
}

/// Darcy friction factor `f(Re, ε/D)`.
///
/// * `Re < 2300` — the exact laminar `f = 64/Re`.
/// * `Re ≥ 4000` — the implicit Colebrook–White equation
///   `1/√f = −2·log10( (ε/D)/3.7 + 2.51/(Re·√f) )`, solved by fixed-point
///   iteration seeded with the explicit Haaland approximation.
/// * `2300 ≤ Re < 4000` — a linear blend between the laminar value at that `Re`
///   and Colebrook evaluated at `Re = 4000`, so `f` is continuous.
///
/// Note the boundary conventions, which are the Java's and are load-bearing:
/// `Re = 2300` exactly is *not* laminar (it enters the blend with weight 0, and
/// so returns the laminar value anyway), and the turbulent evaluation inside the
/// band always uses `max(Re, 4000)`, never the local `Re`.
pub fn friction_factor(re: f64, relative_roughness: f64) -> f64 {
    // Clamp a zero/negative Reynolds number (a transient Newton iterate at
    // near-zero flow) to a tiny positive value instead of throwing: f then
    // stays large and finite (high resistance at vanishing flow), so the
    // solver can step through it rather than crashing.
    let re = if re <= 1.0e-6 { 1.0e-6 } else { re };
    let laminar = 64.0 / re;
    if re < 2300.0 {
        return laminar;
    }
    let turbulent = colebrook(java_max(re, 4000.0), relative_roughness);
    if re < 4000.0 {
        let t = (re - 2300.0) / (4000.0 - 2300.0);
        return laminar + t * (turbulent - laminar);
    }
    turbulent
}

/// Colebrook–White Darcy friction factor (turbulent), iterated to convergence.
///
/// The scheme is transcribed exactly: a Haaland explicit seed, then **at most
/// 60** fixed-point sweeps, returning as soon as successive iterates differ by
/// `≤ 1e-13`. A different seed or tolerance moves the last digits.
fn colebrook(re: f64, relative_roughness: f64) -> f64 {
    let eps = java_max(relative_roughness, 0.0);
    // Haaland explicit initial guess.
    let inv_sqrt = -1.8 * libm::log10(libm::pow(eps / 3.7, 1.11) + 6.9 / re);
    let mut f = 1.0 / (inv_sqrt * inv_sqrt);
    for _ in 0..60 {
        let rhs = -2.0 * libm::log10(eps / 3.7 + 2.51 / (re * libm::sqrt(f)));
        let f_new = 1.0 / (rhs * rhs);
        if libm::fabs(f_new - f) <= 1e-13 {
            return f_new;
        }
        f = f_new;
    }
    f
}

/// Minor (fitting) pressure loss `dP = K · ½ρV²` [Pa].
///
/// `V²` means the loss is unsigned — a fitting resists reversed flow the same
/// way. The Java takes no guards here at all, and neither does this.
pub fn minor_loss(k: f64, rho: f64, velocity: f64) -> f64 {
    k * 0.5 * rho * velocity * velocity
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

    // Water in a 50 mm commercial-steel pipe — the fixture's state.
    const RHO: f64 = 998.0;
    const MU: f64 = 0.001002;
    const D: f64 = 0.05;
    const EPS_D: f64 = 0.0009;

    #[track_caller]
    fn oracle(actual: f64, expected: f64) {
        let diff = (actual - expected).abs();
        assert!(
            diff <= 1e-12 || diff <= 1e-13 * expected.abs().max(actual.abs()),
            "expected {expected}, got {actual} (diff {diff})"
        );
    }

    // --- vs fixtures/golden/flow-resistance-duct.json -----------------------

    #[test]
    fn reynolds_matches_the_oracle_and_ignores_flow_direction() {
        oracle(reynolds(RHO, 1.5, D, MU).unwrap(), 74700.59880239521);
        oracle(reynolds(RHO, -1.5, D, MU).unwrap(), 74700.59880239521);
        oracle(reynolds(RHO, 0.02, D, MU).unwrap(), 996.0079840319362);
        oracle(reynolds(RHO, 0.0, D, MU).unwrap(), 0.0);
    }

    #[test]
    fn turbulent_friction_factor_matches_the_oracle() {
        oracle(
            friction_factor(74700.59880239521, EPS_D),
            0.022536655459594476,
        );
        oracle(friction_factor(74700.59880239521, 0.0), 0.01913498275123008);
        oracle(friction_factor(1e8, EPS_D), 0.01914482199270445);
        oracle(friction_factor(1e6, 0.05), 0.07157375385985786);
        oracle(friction_factor(4000.0, 0.0), 0.03990701405564638);
        oracle(friction_factor(3000.0, 0.0), 0.028981319513109293);
        oracle(friction_factor(3000.0, 0.05), 0.04424948103281862);
        // Roughness only bites above the laminar cut-off: at Re = 2000 the
        // roughest pipe still returns exactly 64/Re.
        oracle(friction_factor(2000.0, 0.05), 0.032);
        // A smooth pipe has less friction than a rough one at the same Re.
        assert!(friction_factor(1e5, 0.0) < friction_factor(1e5, EPS_D));
    }

    #[test]
    fn laminar_friction_factor_is_exactly_64_over_re() {
        oracle(
            friction_factor(996.0079840319362, EPS_D),
            0.0642565130260521,
        );
        oracle(
            friction_factor(996.0079840319362, EPS_D),
            64.0 / 996.0079840319362,
        );
        // Roughness has no effect in the laminar branch.
        oracle(friction_factor(500.0, 0.0), friction_factor(500.0, 0.05));
    }

    #[test]
    fn the_transition_band_is_a_linear_blend_and_stays_continuous() {
        // Re = 2300 is *not* < 2300, so it enters the blend — with weight 0,
        // which lands back on the laminar value. Both sides of the seam agree.
        oracle(friction_factor(2300.0, EPS_D), 0.02782608695652174);
        oracle(friction_factor(2300.0, EPS_D), 64.0 / 2300.0);
        oracle(
            friction_factor(2300.0 - f64::EPSILON * 2300.0, EPS_D),
            64.0 / (2300.0 - f64::EPSILON * 2300.0),
        );
        // Re = 4000 leaves the blend and is pure Colebrook; the blend's top end
        // is the same value, so there is no step there either.
        oracle(friction_factor(3999.999, EPS_D), 0.04081109509960809);
        oracle(friction_factor(4000.0, EPS_D), 0.04081110969437615);
        oracle(friction_factor(4000.001, EPS_D), 0.040811106838558514);
    }

    #[test]
    fn the_transition_band_dips_before_it_rises() {
        // The blend weight rises while the laminar term keeps falling, so f is
        // continuous but *not* monotone across 2300–4000: it dips ~1.3 % below
        // its Re = 2300 value, bottoms out near Re = 2600, and only then climbs
        // to the turbulent value. Every number here is the Java oracle's
        // (fixtures/golden/flow-resistance-transition.json) — the dip is real
        // engine behaviour, not a porting artefact.
        oracle(friction_factor(2300.0, EPS_D), 0.02782608695652174);
        oracle(friction_factor(2400.0, EPS_D), 0.027498692727120168);
        oracle(friction_factor(2600.0, EPS_D), 0.027473453746971358);
        oracle(friction_factor(2800.0, EPS_D), 0.028137721338682063);
        oracle(friction_factor(3000.0, EPS_D), 0.029353594187880375);
        oracle(friction_factor(3500.0, EPS_D), 0.034185993397710904);
        oracle(friction_factor(3900.0, EPS_D), 0.03937576538354558);
        assert!(friction_factor(2600.0, EPS_D) < friction_factor(2300.0, EPS_D));
        // Monotone from the dip onwards.
        let mut prev = friction_factor(2600.0, EPS_D);
        for i in 1..=14 {
            let re = 2600.0 + 100.0 * i as f64;
            let f = friction_factor(re, EPS_D);
            assert!(f > prev, "f({re}) = {f} not above {prev}");
            prev = f;
        }
    }

    #[test]
    fn a_stalled_or_reversed_iterate_gets_a_finite_friction_factor() {
        oracle(friction_factor(0.0, EPS_D), 64000000.0);
        oracle(friction_factor(-25.0, EPS_D), 64000000.0);
        oracle(friction_factor(1e-9, EPS_D), 64000000.0);
        // 1e-6 is the clamp itself, and just above it the law resumes.
        oracle(friction_factor(1e-6, EPS_D), 64000000.0);
        oracle(friction_factor(1e-5, EPS_D), 6400000.0);
    }

    #[test]
    fn minor_loss_matches_the_oracle_and_is_direction_blind() {
        oracle(minor_loss(0.9, RHO, 1.5), 1010.4750000000001);
        oracle(minor_loss(1.0, RHO, 1.5), 1122.75);
        oracle(minor_loss(0.9, RHO, -1.5), 1010.4750000000001);
        oracle(minor_loss(0.9, RHO, 0.0), 0.0);
    }

    // --- Colebrook itself ---------------------------------------------------

    #[test]
    fn colebrook_satisfies_its_own_implicit_equation() {
        for (re, eps) in [
            (4000.0, 0.0),
            (1e5, 0.0009),
            (1e6, 0.05),
            (1e8, 0.0009),
            (1e5, 0.0),
        ] {
            let f = friction_factor(re, eps);
            let residual = 1.0 / f.sqrt() + 2.0 * (eps / 3.7 + 2.51 / (re * f.sqrt())).log10();
            assert!(residual.abs() < 1e-10, "Re={re} eps={eps}: {residual}");
        }
    }

    #[test]
    fn a_negative_roughness_is_treated_as_smooth() {
        oracle(friction_factor(1e5, -0.01), friction_factor(1e5, 0.0));
    }

    // --- domain guard -------------------------------------------------------

    #[test]
    fn reynolds_requires_a_positive_viscosity() {
        for mu in [0.0, -1.0] {
            let e = reynolds(RHO, 1.5, D, mu).unwrap_err().to_string();
            assert!(e.contains("dynamic viscosity must be > 0"), "{e}");
        }
        assert!(matches!(
            reynolds(RHO, 1.5, D, 0.0),
            Err(FreesError::Property { .. })
        ));
    }
}
