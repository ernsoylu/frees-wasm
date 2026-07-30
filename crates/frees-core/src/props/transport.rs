//! Transport properties of ideal-gas mixtures from kinetic theory.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/props/GasTransport.java`
//! (105 LOC), in full.
//!
//! Pure-species dynamic viscosity comes from the Chapman–Enskog relation with
//! the Neufeld collision integral; thermal conductivity from the Eucken
//! relation on top of it. Mixtures are combined with Wilke's rule (Mason–Saxena
//! for conductivity — the *same* interaction coefficients `phi_ij`, which is
//! why one routine serves both).
//!
//! This covers arbitrary ideal-gas mixtures (air, combustion products) that
//! CoolProp does not handle as mixtures. Composition uses the same
//! `'species:amount, ...'` string as the `mix_*` property functions, parsed by
//! [`crate::props::thermochem::composition`]; the Lennard-Jones parameters come
//! from the combustion-mechanism transport data in [`crate::props::nasa`].
//!
//! Outputs are SI: viscosity [Pa·s], conductivity [W/m·K].
//!
//! # Summation order is part of the answer
//!
//! `composition` returns mole fractions in **first-seen order**, and the Wilke
//! double sum accumulates in that order. Java iterates a `LinkedHashMap` for
//! the same reason. Reordering the components changes the last digits of a
//! floating-point sum, so the ordered `Vec` is not an implementation detail.

use crate::diag::{FreesError, Result};
use crate::props::{nasa, thermochem};

/// Universal gas constant [J/mol·K] — the Java's own literal, local to this
/// file exactly as `GasTransport.R` is.
const R: f64 = 8.314462618;

/// Reduced collision integral `Ω(2,2)*` (Neufeld et al., 1972) at
/// `T* = T/(ε/k)`.
///
/// The three-term fit's coefficients are the paper's truncated literals,
/// transcribed verbatim from the Java.
pub fn collision_integral(t_star: f64) -> f64 {
    1.16145 * libm::pow(t_star, -0.14874)
        + 0.52487 * libm::exp(-0.77320 * t_star)
        + 2.16178 * libm::exp(-2.43787 * t_star)
}

/// Pure-species dynamic viscosity [Pa·s] (Chapman–Enskog):
/// `mu = 2.6693e-6·√(M[g/mol]·T) / (σ[Å]²·Ω)`.
pub fn viscosity(species: &str, t: f64) -> Result<f64> {
    require_transport(species)?;
    let m_gmol = nasa::molar_mass(species)?; // g/mol
    let sigma = nasa::collision_diameter(species)?; // Angstrom
    let t_star = t / nasa::well_depth(species)?;
    Ok(2.6693e-6 * libm::sqrt(m_gmol * t) / (sigma * sigma * collision_integral(t_star)))
}

/// Pure-species thermal conductivity [W/m·K] (Eucken relation):
/// `k = (mu/M[kg/mol])·(cp_molar + 1.25·R)`.
pub fn conductivity(species: &str, t: f64) -> Result<f64> {
    let mu = viscosity(species, t)?;
    let cp_molar = nasa::molar_cp(species, t)?; // J/mol-K
    let m_kgmol = nasa::molar_mass(species)? / 1000.0; // kg/mol
    Ok((mu / m_kgmol) * (cp_molar + 1.25 * R))
}

/// Mixture dynamic viscosity [Pa·s] via Wilke's rule.
pub fn mixture_viscosity(comp: &str, t: f64) -> Result<f64> {
    let cs = components(comp, t)?;
    Ok(wilke_average(&cs, true))
}

/// Mixture thermal conductivity [W/m·K] via the Wilke / Mason–Saxena rule.
pub fn mixture_conductivity(comp: &str, t: f64) -> Result<f64> {
    let cs = components(comp, t)?;
    Ok(wilke_average(&cs, false))
}

/// Per-species data needed for the mixing rule.
struct Component {
    /// Mole fraction.
    x: f64,
    /// Molar mass [g/mol].
    mw: f64,
    /// Dynamic viscosity [Pa·s].
    mu: f64,
    /// Thermal conductivity [W/m·K].
    k: f64,
}

/// Resolves the composition string into per-species transport data.
///
/// Both `mu` and `k` are evaluated for every species even when only one of them
/// is wanted, exactly as the Java's `components` does — the wasted `molar_cp`
/// call is also the reason a species with LJ data but no usable NASA-7 range
/// would fail identically from either entry point.
fn components(comp: &str, t: f64) -> Result<Vec<Component>> {
    let xs = thermochem::composition(comp)?;
    let mut list = Vec::with_capacity(xs.len());
    for (sp, x) in &xs {
        require_transport(sp)?;
        list.push(Component {
            x: *x,
            mw: nasa::molar_mass(sp)?,
            mu: viscosity(sp, t)?,
            k: conductivity(sp, t)?,
        });
    }
    Ok(list)
}

/// Wilke average of the per-species property (viscosity when `visc`, else
/// conductivity), sharing the same interaction coefficients `phi_ij`:
///
/// ```text
///   phi_ij = [1 + sqrt(mu_i/mu_j) * (M_j/M_i)^0.25]^2 / sqrt(8*(1 + M_i/M_j))
///   result = sum_i  x_i * p_i / sum_j x_j * phi_ij
/// ```
fn wilke_average(cs: &[Component], visc: bool) -> f64 {
    let mut sum = 0.0;
    for i in cs {
        let mut denom = 0.0;
        for j in cs {
            let ratio = i.mu / j.mu;
            let mij = i.mw / j.mw;
            let num = 1.0 + libm::sqrt(ratio) * libm::pow(1.0 / mij, 0.25);
            let phi = num * num / libm::sqrt(8.0 * (1.0 + mij));
            denom += j.x * phi;
        }
        sum += i.x * (if visc { i.mu } else { i.k }) / denom;
    }
    sum
}

fn require_transport(species: &str) -> Result<()> {
    if !nasa::has_transport(species) {
        return Err(FreesError::property(format!(
            "Transport: no Lennard-Jones data for species '{species}'. Known: the standard \
             combustion-mechanism species (N2, O2, CO2, H2O, CH4, ...)."
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn oracle(actual: f64, expected: f64) {
        let diff = (actual - expected).abs();
        assert!(
            diff <= 1e-12 || diff <= 1e-13 * expected.abs().max(actual.abs()),
            "expected {expected}, got {actual} (diff {diff})"
        );
    }

    // --- vs fixtures/corpus-pending/golden/gas-transport-mixture.json -------
    //
    // The oracle exposes only `mix_viscosity` / `mix_conductivity`, so the
    // pure-species values below are read off single-species mixtures. That is
    // exact, not an approximation: for one component Wilke's phi_ii is
    // (1 + 1·1)² / √16 = 1, so the mixture value *is* the pure value.

    #[test]
    fn pure_species_viscosity_matches_the_oracle_at_300k() {
        oracle(viscosity("N2", 300.0).unwrap(), 1.8075004765988316e-05);
        oracle(viscosity("O2", 300.0).unwrap(), 2.0636682929874117e-05);
        oracle(viscosity("CO2", 300.0).unwrap(), 1.5072441159712493e-05);
        oracle(viscosity("H2O", 300.0).unwrap(), 1.296151365644123e-05);
        oracle(viscosity("CH4", 300.0).unwrap(), 1.1450093612533707e-05);
        oracle(viscosity("AR", 300.0).unwrap(), 2.313201504380884e-05);
    }

    #[test]
    fn pure_species_conductivity_matches_the_oracle_at_300k() {
        oracle(conductivity("N2", 300.0).unwrap(), 0.025465639325001448);
        oracle(conductivity("O2", 300.0).unwrap(), 0.025656321233996724);
        oracle(conductivity("CO2", 300.0).unwrap(), 0.016306013818816707);
        oracle(conductivity("H2O", 300.0).unwrap(), 0.03164978574312276);
        oracle(conductivity("CH4", 300.0).unwrap(), 0.03294042246316525);
        oracle(conductivity("AR", 300.0).unwrap(), 0.018054433985502615);
    }

    #[test]
    fn a_single_species_mixture_is_the_pure_species_value() {
        for sp in ["N2", "O2", "CO2", "H2O", "CH4", "AR"] {
            let spec = format!("{sp}:1");
            assert_eq!(
                mixture_viscosity(&spec, 300.0).unwrap(),
                viscosity(sp, 300.0).unwrap()
            );
            assert_eq!(
                mixture_conductivity(&spec, 300.0).unwrap(),
                conductivity(sp, 300.0).unwrap()
            );
        }
    }

    #[test]
    fn air_matches_the_oracle_cold_and_hot() {
        oracle(
            mixture_viscosity("N2:0.79, O2:0.21", 300.0).unwrap(),
            1.861838941354871e-05,
        );
        oracle(
            mixture_conductivity("N2:0.79, O2:0.21", 300.0).unwrap(),
            0.025512742570156198,
        );
        oracle(
            mixture_viscosity("N2:0.79, O2:0.21", 1200.0).unwrap(),
            4.820581368536363e-05,
        );
        oracle(
            mixture_conductivity("N2:0.79, O2:0.21", 1200.0).unwrap(),
            0.07436134990350689,
        );
    }

    #[test]
    fn combustion_products_match_the_oracle() {
        oracle(
            mixture_viscosity("CO2:0.10, H2O:0.18, N2:0.72", 1200.0).unwrap(),
            4.735262847373853e-05,
        );
        oracle(
            mixture_conductivity("CO2:0.10, H2O:0.18, N2:0.72", 1200.0).unwrap(),
            0.08446605811716318,
        );
    }

    #[test]
    fn the_mixture_lies_between_its_components() {
        // Wilke is not a mole-fraction average, but it is bounded by the
        // pure-species values for a binary — a cheap sanity net on the double
        // sum's indexing.
        let mu = mixture_viscosity("N2:0.79, O2:0.21", 300.0).unwrap();
        let lo = viscosity("N2", 300.0).unwrap();
        let hi = viscosity("O2", 300.0).unwrap();
        assert!(lo < mu && mu < hi, "{lo} < {mu} < {hi}");
    }

    #[test]
    fn composition_is_normalized_so_scale_does_not_matter() {
        let a = mixture_viscosity("N2:0.79, O2:0.21", 300.0).unwrap();
        let b = mixture_viscosity("N2:79, O2:21", 300.0).unwrap();
        let c = mixture_viscosity("N2:3.7619047619047619, O2:1", 300.0).unwrap();
        assert_eq!(a, b);
        assert!((a - c).abs() < 1e-18, "{a} vs {c}");
    }

    #[test]
    fn species_names_are_case_insensitive() {
        assert_eq!(
            viscosity("n2", 300.0).unwrap(),
            viscosity("N2", 300.0).unwrap()
        );
        // NasaThermo maps the long spelling of argon onto AR.
        assert_eq!(
            viscosity("argon", 300.0).unwrap(),
            viscosity("AR", 300.0).unwrap()
        );
    }

    // --- the collision integral itself --------------------------------------

    #[test]
    fn collision_integral_falls_monotonically_with_reduced_temperature() {
        let mut prev = collision_integral(0.3);
        for i in 1..60 {
            let t_star = 0.3 + 0.5 * i as f64;
            let omega = collision_integral(t_star);
            assert!(omega < prev, "Omega({t_star}) = {omega} not below {prev}");
            assert!(omega > 0.0);
            prev = omega;
        }
    }

    #[test]
    fn collision_integral_reproduces_the_neufeld_fit() {
        // Spot values of the three-term fit, recomputed term by term.
        for t_star in [0.5f64, 1.0, 3.0763, 12.31] {
            let expected = 1.16145 * t_star.powf(-0.14874)
                + 0.52487 * (-0.77320 * t_star).exp()
                + 2.16178 * (-2.43787 * t_star).exp();
            assert!((collision_integral(t_star) - expected).abs() < 1e-15);
        }
    }

    // --- domain guards ------------------------------------------------------

    #[test]
    fn species_without_lennard_jones_data_are_refused() {
        let e = viscosity("R134a", 300.0).unwrap_err().to_string();
        assert!(
            e.contains("no Lennard-Jones data for species 'R134a'"),
            "{e}"
        );
        assert!(e.contains("N2, O2, CO2, H2O, CH4"), "{e}");
        assert!(conductivity("R134a", 300.0).is_err());
        let e = mixture_viscosity("N2:0.5, R134a:0.5", 300.0)
            .unwrap_err()
            .to_string();
        assert!(e.contains("no Lennard-Jones data"), "{e}");
        assert!(mixture_conductivity("N2:0.5, R134a:0.5", 300.0).is_err());
    }

    #[test]
    fn a_malformed_composition_is_refused_by_the_shared_parser() {
        assert!(mixture_viscosity("N2 0.79", 300.0).is_err());
        assert!(mixture_viscosity("N2:0", 300.0).is_err());
        assert!(mixture_conductivity("", 300.0).is_err());
    }

    #[test]
    fn errors_are_property_evaluation_failures() {
        assert!(matches!(
            viscosity("R134a", 300.0),
            Err(FreesError::Property { .. })
        ));
    }
}
